"""Exercise real desktop sidecars against isolated native stores; no model calls.

Usage: python desktop/tests/multi_instance.py path/to/codewhale.exe
"""
import json
import os
import pathlib
import queue
import subprocess
import sys
import tempfile
import threading


class Sidecar:
    def __init__(self, binary, home, workspace, scope):
        self.scope = home / 'tasks' / ('runtime' if scope == 'legacy' else f'desktop/{scope}')
        self.scope.mkdir(parents=True, exist_ok=True)
        config = home / f'{scope}.toml'
        config.write_text('provider="deepseek"\nmodel="deepseek-flash"\nbase_url="http://127.0.0.1:9"\ntelemetry=false\n[features]\nmcp=false\n', encoding='utf-8')
        env = {k: v for k, v in os.environ.items() if k.upper() in ('PATH', 'SYSTEMROOT', 'WINDIR', 'TEMP', 'TMP', 'HOME', 'USERPROFILE', 'LOCALAPPDATA', 'APPDATA')}
        env.update(CODEWHALE_HOME=str(home), CODEWHALE_RUNTIME_DIR=str(self.scope),
                   CODEWHALE_DESKTOP_SUPERVISED='1', CODEWHALE_TELEMETRY='0',
                   DEEPSEEK_API_KEY='local-test-not-a-secret')
        self.process = subprocess.Popen([str(binary), 'app-server', '--stdio', '--config', str(config)],
                                        cwd=workspace, env=env, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                        text=True, encoding='utf-8',
                                        creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
        self.replies = queue.Queue()
        self.sequence = 0
        def read():
            for line in self.process.stdout:
                value = json.loads(line)
                if 'id' in value:
                    self.replies.put(value)
            self.replies.put(None)
        threading.Thread(target=read, daemon=True).start()
        try:
            assert self.call('capabilities')['desktop_multi_instance'] is True
            self.thread = self.call('thread/start', {'cwd': str(workspace)})['thread_id']
        except BaseException:
            self.close()
            raise

    def call(self, method, params=None):
        self.sequence += 1
        self.process.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': self.sequence,
                                            'method': method, 'params': params or {}}) + '\n')
        self.process.stdin.flush()
        reply = self.replies.get(timeout=35)
        assert reply is not None and reply['id'] == self.sequence, reply
        if 'error' in reply:
            raise RuntimeError(reply['error'])
        return reply['result']

    def session(self, operation='new', **params):
        return self.call('desktop/session', {'thread_id': self.thread, 'operation': operation, **params})

    def close(self):
        if self.process.poll() is None:
            try:
                self.call('shutdown')
                self.process.wait(timeout=10)
            except (RuntimeError, AssertionError, queue.Empty, OSError, subprocess.TimeoutExpired):
                pass
            finally:
                if self.process.poll() is None:
                    if os.name == 'nt':
                        subprocess.run(['taskkill', '/PID', str(self.process.pid), '/T', '/F'],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
                    else:
                        self.process.kill()
                    self.process.wait()


def main():
    binary = pathlib.Path(sys.argv[1]).resolve()
    peers = []
    with tempfile.TemporaryDirectory(prefix='medsci-multi-') as temporary:
        root = pathlib.Path(temporary)
        home = root / 'agent'
        same = root / '同一个目录 with spaces'
        other = root / 'another-workspace'
        same.mkdir()
        other.mkdir()
        def launch(scope, workspace=same):
            peer = Sidecar(binary, home, workspace, scope)
            peers.append(peer)
            return peer
        try:
            legacy = launch('legacy')
            legacy_id = legacy.session()['runtime_id']
            legacy.close()
            first, second, third = launch('session-first'), launch('session-second'), launch('session-third', other)
            ids = [peer.session()['runtime_id'] for peer in (first, second, third)]
            assert len(set(ids)) == 3
            history = second.session('list')
            assert {item['id'] for item in history} == {ids[0], ids[1], legacy_id}, history
            assert {item['store'] for item in history} == {'session-first', 'session-second', 'legacy'}
            assert {item['id'] for item in third.session('list')} == {ids[2]}
            print('PASS: same/different workspaces, independent sessions, shared native history and legacy history')
            competing = launch('session-first')
            try:
                competing.session()
            except RuntimeError as error:
                assert 'runtime' in str(error).lower(), error
            else:
                raise AssertionError('a second Runtime acquired an occupied store')
            competing.close()
            assert first.session('read', runtime_id=ids[0])['runtime_id'] == ids[0]
            print('PASS: native store ownership rejects competing writers without disrupting the owner')
            first.close()
            assert second.session('read', runtime_id=ids[1])['runtime_id'] == ids[1]
            resumed = launch('session-first')
            assert resumed.session('read', runtime_id=ids[0])['runtime_id'] == ids[0]
            assert (resumed.scope / 'threads' / f'{ids[0]}.json').is_file()
            print('PASS: independent shutdown, persistent history and resume after release')
        finally:
            for peer in reversed(peers):
                peer.close()
    print('3 scenario groups passed; 0 failed; no model/provider calls')


if __name__ == '__main__':
    main()
