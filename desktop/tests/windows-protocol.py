"""Run the Windows sidecar handshake in an isolated Wine prefix; no model calls."""
import json,os,pathlib,subprocess,tempfile
root=pathlib.Path(__file__).resolve().parents[2]
agent=root/'desktop/artifacts/stage-windows-x86_64/resources/codewhale.exe'
def win(path):return 'Z:'+str(path.resolve()).replace('/','\\')
with tempfile.TemporaryDirectory(prefix='win-protocol-',dir=root/'desktop/artifacts') as temporary:
    folder=pathlib.Path(temporary)
    config=folder/'config.toml';config.write_text('provider="deepseek"\ntelemetry=false\n[features]\nmcp=false\n')
    requests=[{'jsonrpc':'2.0','id':i,'method':method,'params':{'cwd':win(folder)} if method=='thread/start' else {}} for i,method in enumerate(['healthz','capabilities','thread/start','shutdown'],1)]
    (folder/'input.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in requests))
    batch=folder/'smoke.cmd'
    batch.write_text('@echo off\nset "CODEWHALE_HOME='+win(folder/'state')+'"\nset "CODEWHALE_TELEMETRY=0"\n"'+win(agent)+'" app-server --stdio --config "'+win(config)+'" < "'+win(folder/'input.jsonl')+'" > "'+win(folder/'output.jsonl')+'" 2> "'+win(folder/'error.log')+'"\nexit /b %ERRORLEVEL%\n')
    env={k:v for k,v in os.environ.items() if k in ['PATH','HOME','TMPDIR','USER','LOGNAME']}
    env.update(WINEPREFIX=str(root/'desktop/test-wine'),WINEDEBUG='-all')
    completed=subprocess.run(['wine','cmd','/d','/c',win(batch)],env=env,capture_output=True,timeout=60)
    assert completed.returncode==0,(completed.returncode,(folder/'error.log').read_text(errors='replace'))
    replies={v['id']:v for line in (folder/'output.jsonl').read_text().splitlines() if (v:=json.loads(line)).get('id')}
    assert replies[1]['result']['version']=='0.9.13',replies
    assert {'desktop/session','thread/steer','desktop/approval'}.issubset(replies[2]['result']['methods']),replies[2]
    assert replies[3]['result']['thread_id'],replies[3]
    print('PASS under Wine: packaged Windows x64 sidecar health, native session/steer capabilities, thread/start, shutdown; no model/provider calls')
