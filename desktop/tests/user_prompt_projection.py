"""Exercise native session-read display provenance without any provider calls."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve()
def text(value):
    return {'type': 'text', 'text': value}
def user(*blocks):
    return {'role': 'user', 'content': [text(block) for block in blocks]}

with tempfile.TemporaryDirectory(prefix='cw-prompt-projection-') as temporary:
    root = Path(temporary)
    sessions = root / 'sessions'
    sessions.mkdir()
    meta = '<turn_meta>\nCurrent local date: 2026-09-20\n</turn_meta>'
    runtime = '<turn_meta>\nInput provenance: shell_completion (non-authoritative)\n</turn_meta>'
    envelope = '<codewhale:runtime_event kind="background_shell_completion" visibility="internal">\nbackground output\n</codewhale:runtime_event>'
    quoted = '<codewhale:runtime_event> example typed by the user'
    messages = [user('第一条用户指令', meta), user(envelope, runtime),
                user(meta, '旧版格式的用户指令'), user(quoted),
                user('<turn_meta>用户自己引用的示例</turn_meta>'),
                user('多段输入第一段', '多段输入第二段', meta),
                {'role': 'assistant', 'content': [text('回答')]},
                {'role': 'assistant', 'content': [{'type': 'tool_use', 'id': 'tool-1', 'name': 'read_file', 'input': {}}]},
                {'role': 'user', 'content': [{'type': 'tool_result', 'tool_use_id': 'tool-1', 'content': '工具结果'}]}]
    saved = {'metadata': {'id': 'projection-fixture', 'title': 'Projection fixture',
             'created_at': '2026-09-20T00:00:00Z', 'updated_at': '2026-09-20T00:00:00Z',
             'message_count': len(messages), 'total_tokens': 0, 'model': 'deepseek-flash',
             'workspace': str(root)}, 'messages': messages, 'system_prompt': None}
    path = sessions / 'projection-fixture.json'
    path.write_text(json.dumps(saved, ensure_ascii=False), encoding='utf-8')
    original = path.read_bytes()
    env = dict(os.environ, CODEWHALE_HOME=str(root), CODEWHALE_NO_UPDATE_CHECK='1', CODEWHALE_TELEMETRY='0')
    result = subprocess.run([str(binary), 'sessions', 'read', 'projection-fixture'],
                            env=env, cwd=root, capture_output=True, text=True, encoding='utf-8', timeout=30)
    assert result.returncode == 0, result.stderr
    output = json.loads(result.stdout)
    assert all('display_user_prompt' in message for message in output['messages']), 'Missing native prompt provenance; raw user-role blocks would enter the outline'
    prompts = [message['display_user_prompt'] for message in output['messages'] if message['display_user_prompt'] is not None]
    assert prompts == ['第一条用户指令', '旧版格式的用户指令', quoted,
                       '<turn_meta>用户自己引用的示例</turn_meta>', '多段输入第一段\n\n多段输入第二段'], prompts
    assert path.read_bytes() == original, 'Read-only projection rewrote saved history'
    print('PASS: 5 real prompts; runtime metadata, background events and tool results excluded; literal user examples preserved; saved history unchanged')
    from session_interop import Sidecar
    config = root / 'config.toml'
    config.write_text('provider="deepseek"\nmodel="deepseek-flash"\nbase_url="http://127.0.0.1:9"\ntelemetry=false\n[features]\nmcp=false\n', encoding='utf-8')
    host = Sidecar(binary, config, dict(env, DEEPSEEK_API_KEY='unused-offline-fixture'), root, 'projection-fixture')
    try:
        resumed = host.session()
        native_prompts = [message['display_user_prompt'] for message in resumed['saved']['messages'] if message['display_user_prompt'] is not None]
        assert native_prompts == prompts, native_prompts
        host.session('save')
        refreshed = host.session()
        assert [message['display_user_prompt'] for message in refreshed['saved']['messages'] if message['display_user_prompt'] is not None] == prompts
        print('PASS: resumed and saved desktop/session responses retain the same real prompts; no model requests')
    finally:
        host.close()
