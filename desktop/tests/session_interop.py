"""Native session/stdio integration in disposable homes with a loopback provider.

Usage: python desktop/tests/session_interop.py target/debug/codewhale.exe
Pass --tui on Windows to alternate real interactive TUI and stdio hosts.
This is not a receipt for GUI window rendering or packaged installation.
"""
import http.server
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import time


class Provider(http.server.BaseHTTPRequestHandler):
    requests = []

    def log_message(self, *_args):
        pass

    def do_GET(self):
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(b'{"data":[{"id":"deepseek-flash"}]}')

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        self.requests.append(body)
        self.send_response(200)
        if body.get('stream') is not True:
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps({'id': 'fixture', 'model': 'deepseek-flash',
                'choices': [{'index': 0, 'message': {'role': 'assistant', 'content': 'Local conversation summary.'},
                             'finish_reason': 'stop'}],
                'usage': {'prompt_tokens': 100, 'completion_tokens': 10, 'total_tokens': 110}}).encode())
            return
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        for delta in ({'reasoning_content': 'local reasoning'}, {'content': 'local answer'}):
            frame = {'id': 'fixture', 'model': 'deepseek-flash', 'choices': [
                {'index': 0, 'delta': delta, 'finish_reason': None}]}
            self.wfile.write(('data: ' + json.dumps(frame) + '\n\n').encode())
        self.wfile.write(b'data: {"id":"fixture","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":10,"total_tokens":110}}\n\ndata: [DONE]\n\n')
        self.wfile.flush()


class Sidecar:
    def __init__(self, binary, config, env, workspace, session_id):
        self.id = session_id
        self.process = subprocess.Popen(
            [str(binary), 'app-server', '--stdio', '--config', str(config)],
            cwd=workspace, env=dict(env, CODEWHALE_SESSION_ID=session_id),
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            text=True, encoding='utf-8', creationflags=0x08000000 if os.name == 'nt' else 0)
        self.replies = queue.Queue()
        self.sequence = 0

        def read():
            for line in self.process.stdout:
                value = json.loads(line)
                if 'id' in value:
                    self.replies.put(value)
            self.replies.put(None)

        threading.Thread(target=read, daemon=True).start()
        assert self.call('capabilities')['native_session_interop'] is True
        self.thread = self.call('thread/start', {'cwd': str(workspace), 'model': 'deepseek-flash'})['thread_id']

    def call(self, method, params=None):
        self.sequence += 1
        self.process.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': self.sequence,
                                             'method': method, 'params': params or {}}) + '\n')
        self.process.stdin.flush()
        reply = self.replies.get(timeout=60)
        assert reply is not None and reply['id'] == self.sequence, reply
        if 'error' in reply:
            raise RuntimeError(reply['error'])
        return reply['result']

    def session(self, operation='read'):
        return self.call('desktop/session', {'thread_id': self.thread, 'session_id': self.id,
                                              'operation': operation})

    def close(self):
        if self.process.poll() is None:
            self.call('shutdown')
            self.process.wait(timeout=20)


def main():
    binary = Path(sys.argv[1]).resolve()
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Provider)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    with tempfile.TemporaryDirectory(prefix='cw-session-interop-') as temporary:
        root = Path(temporary)
        home, workspace = root / 'home', root / '中文 workspace'
        home.mkdir()
        workspace.mkdir()
        config = root / 'private.toml'
        config.write_text('provider="deepseek"\nmodel="deepseek-flash"\n'
                          f'base_url="http://127.0.0.1:{server.server_port}"\n'
                          'telemetry=false\n[features]\nmcp=false\nsubagents=false\n', encoding='utf-8')
        env = {k: v for k, v in os.environ.items() if k.upper() in
               ('PATH', 'SYSTEMROOT', 'SYSTEMDRIVE', 'WINDIR', 'TEMP', 'TMP', 'HOME', 'USERPROFILE', 'LOCALAPPDATA', 'APPDATA')}
        env.update(CODEWHALE_HOME=str(home), CODEWHALE_TELEMETRY='0',
                   CODEWHALE_NO_UPDATE_CHECK='1', DEEPSEEK_API_KEY='local-fixture')
        readers = dict(env)
        readers.pop('DEEPSEEK_API_KEY')

        def native(*args):
            result = subprocess.run([str(binary), 'sessions', *args], env=readers, cwd=workspace,
                                    capture_output=True, text=True, encoding='utf-8', timeout=30,
                                    creationflags=0x08000000 if os.name == 'nt' else 0)
            assert result.returncode == 0, result.stderr
            return json.loads(result.stdout)

        def tui_turn(session_id, prompt):
            from winpty import PtyProcess
            terminal = PtyProcess.spawn(
                [str(binary), '--config', str(config), '--skip-onboarding',
                 *(['--resume', session_id] if session_id else ['--fresh'])],
                cwd=str(workspace), env=env, dimensions=(32, 100))
            transcript = []

            def read_terminal():
                try:
                    while terminal.isalive():
                        transcript.append(terminal.read(65536))
                except EOFError:
                    pass

            threading.Thread(target=read_terminal, daemon=True).start()
            previous = len(Provider.requests)
            try:
                time.sleep(3)
                terminal.write(prompt)
                time.sleep(0.3)
                terminal.write('\r')
                deadline = time.monotonic() + 45
                while len(Provider.requests) == previous and terminal.isalive() and time.monotonic() < deadline:
                    time.sleep(0.1)
                assert len(Provider.requests) == previous + 1, ''.join(transcript)[-10000:]
                time.sleep(2)
                terminal.write('/exit')
                time.sleep(0.3)
                terminal.write('\r')
                deadline = time.monotonic() + 20
                while terminal.isalive() and time.monotonic() < deadline:
                    time.sleep(0.1)
                assert not terminal.isalive(), ''.join(transcript)[-10000:]
            finally:
                if terminal.isalive():
                    terminal.terminate(force=True)
                terminal.close()
            return ''.join(transcript)

        sid = native('allocate')['id']
        assert native('list', '--json', '--workspace', str(workspace)) == []
        peers = []
        try:
            first = Sidecar(binary, config, env, workspace, sid)
            peers.append(first)
            created = first.session('new')
            runtime_id = created['runtime_id']
            assert created['session_id'] == sid and runtime_id != sid
            other_id = native('allocate')['id']
            other = Sidecar(binary, config, env, workspace, other_id)
            peers.append(other)
            other.session('new')
            assert {row['id'] for row in native('list', '--json', '--workspace', str(workspace))} == {sid, other_id}
            competitor = Sidecar(binary, config, env, workspace, sid)
            peers.append(competitor)
            try:
                competitor.session()
            except RuntimeError:
                pass
            else:
                raise AssertionError('another host acquired an occupied native session')
            competitor.close()
            assert native('read', sid)['metadata']['id'] == sid
            print('PASS: native identities, keyless readers, parallel sessions, exclusive owner', flush=True)
            first.close()
            for turn in range(4):
                if '--tui' in sys.argv and turn % 2 == 1:
                    tui_turn(sid, f'ROUND_{turn}')
                    saved = native('read', sid)
                    users = [message for message in saved['messages'] if message['role'] == 'user']
                    assert len(users) == turn + 1, (turn, saved['messages'])
                    continue
                host = Sidecar(binary, config, env, workspace, sid)
                peers.append(host)
                assert host.session()['runtime_id'] == runtime_id
                host.call('thread/message', {'thread_id': host.thread, 'input': f'ROUND_{turn}'})
                host.session('save')
                accounting = native('read', sid)['metadata']
                host.session('save')
                repeated = native('read', sid)['metadata']
                assert repeated['cost'] == accounting['cost'], (accounting, repeated)
                assert repeated['total_tokens'] == accounting['total_tokens']
                host.close()
                saved = native('read', sid)
                users = [message for message in saved['messages'] if message['role'] == 'user']
                assert len(users) == turn + 1, (turn, saved['messages'])
                assert saved['metadata']['runtime_store']['data_dir']
                if '--tui' not in sys.argv:
                    assert saved['metadata']['total_tokens'] == (turn + 1) * 110, saved['metadata']
            assert len(Provider.requests) == 4
            assert all(f'ROUND_{turn}' in json.dumps(Provider.requests[-1]) for turn in range(4))
            assert len(native('list', '--json', '--workspace', str(workspace))) == 2
            final = Sidecar(binary, config, env, workspace, sid)
            peers.append(final)
            assert final.session()['runtime_id'] == runtime_id
            final.close()
            print('PASS: four real process turns retain history and one SavedSession/Runtime identity', flush=True)
            if '--compact' in sys.argv:
                saved = native('read', sid)
                saved_path = home / 'sessions' / f'{sid}.json'
                old_snapshot_bytes = saved_path.read_bytes()
                record_path = Path(saved['metadata']['runtime_store']['data_dir']) / 'threads' / f'{runtime_id}.json'
                before = json.loads(record_path.read_text(encoding='utf-8'))['saved_session_checkpoint']
                compaction_transcript = tui_turn(sid, '/compact')
                compacted = native('read', sid)
                compacted_snapshot_bytes = saved_path.read_bytes()
                assert compacted['messages'] != saved['messages'], compaction_transcript[-12000:]
                record = json.loads(record_path.read_text(encoding='utf-8'))
                committed = record['saved_session_checkpoint']
                assert committed['messages_sha256'] != before['messages_sha256']
                # Fault injection into this disposable store: reproduce a crash
                # after SavedSession rename and before checkpoint confirmation.
                pending = dict(before, pending={
                    'messages_sha256': committed['messages_sha256'],
                    'message_count': committed['message_count'],
                    'covered_turn_id': committed['covered_turn_id']})
                record['saved_session_checkpoint'] = pending
                record_path.write_text(json.dumps(record, ensure_ascii=False), encoding='utf-8')
                # Before the atomic snapshot rename, the old published state
                # is still authoritative even with a newer pending digest.
                saved_path.write_bytes(old_snapshot_bytes)
                failed_write = Sidecar(binary, config, env, workspace, sid)
                peers.append(failed_write)
                assert failed_write.session()['runtime_id'] == runtime_id
                failed_write.close()
                assert native('read', sid)['messages'] == saved['messages']
                saved_path.write_bytes(compacted_snapshot_bytes)
                recovery = Sidecar(binary, config, env, workspace, sid)
                peers.append(recovery)
                assert recovery.session()['runtime_id'] == runtime_id
                recovery.session('save')
                recovery.close()
                recovered_messages = native('read', sid)['messages']
                # Native restore deliberately demotes the old live Agent
                # topology to a historical checkpoint. Conversation content,
                # reasoning and the compaction summary must remain identical.
                assert len(recovered_messages) == len(compacted['messages'])
                assert recovered_messages[:-1] == compacted['messages'][:-1], (compacted['messages'], recovered_messages)
                assert 'agent_topology' in compacted['messages'][-1]['content'][0]['text']
                topology = recovered_messages[-1]['content'][0]['text']
                assert 'historical runtime checkpoint' in topology and 'total=0' in topology, topology
                record = json.loads(record_path.read_text(encoding='utf-8'))
                assert record['saved_session_checkpoint'].get('pending') is None
                stable = dict(record)
                pending['pending']['messages_sha256'] = '0' * 64
                record['saved_session_checkpoint'] = pending
                record_path.write_text(json.dumps(record, ensure_ascii=False), encoding='utf-8')
                divergent = Sidecar(binary, config, env, workspace, sid)
                peers.append(divergent)
                try:
                    divergent.session()
                except RuntimeError:
                    pass
                else:
                    raise AssertionError('unproven compaction digest was accepted')
                divergent.close()
                record_path.write_text(json.dumps(stable, ensure_ascii=False), encoding='utf-8')
                print('PASS: TUI compaction, interrupted snapshot publication recovery, and divergence refusal', flush=True)
            if '--tui' in sys.argv:
                tui_turn(None, 'CREATED_IN_TUI')
                rows = native('list', '--json', '--workspace', str(workspace))
                new_ids = {row['id'] for row in rows} - {sid, other_id}
                assert len(new_ids) == 1, rows
                native_id = new_ids.pop()
                config.write_text(config.read_text(encoding='utf-8').replace(
                    'telemetry=false',
                    'telemetry=false\napproval_policy="auto"\nsandbox_mode="danger-full-access"'),
                    encoding='utf-8')
                from_tui = Sidecar(binary, config, env, workspace, native_id)
                peers.append(from_tui)
                resumed = from_tui.session()
                assert resumed['session_id'] == native_id
                assert resumed['detail']['thread']['permission_posture'] != 'full_access', resumed
                assert resumed['detail']['thread']['auto_approve'] is False, resumed
                from_tui.call('thread/message', {'thread_id': from_tui.thread, 'input': 'GUI_AFTER_TUI'})
                from_tui.session('save')
                from_tui.close()
                saved = native('read', native_id)
                assert sum(message['role'] == 'user' for message in saved['messages']) == 2
                print('PASS: TUI-created native session resumes and saves through desktop transport', flush=True)
            other.session()
        finally:
            for peer in reversed(peers):
                peer.close()
    server.shutdown()
    groups = 2 + ('--tui' in sys.argv) + ('--compact' in sys.argv)
    print(f'{groups} scenario groups passed; 0 failed; only a loopback fixture provider was used')


if __name__ == '__main__':
    main()
