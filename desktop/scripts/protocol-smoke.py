"""No provider credentials and no model calls: real packaged stdio handshake."""
import json,os,pathlib,subprocess,sys,tempfile
binary=pathlib.Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix='medsci-protocol-') as temp:
 root=pathlib.Path(temp);config=root/'config.toml';config.write_text('provider="deepseek"\ntelemetry=false\n[features]\nmcp=false\n')
 env={k:v for k,v in os.environ.items() if k in ['PATH','HOME','SystemRoot','WINDIR','TEMP','TMP','USERPROFILE','LOCALAPPDATA','APPDATA']}
 env['CODEWHALE_HOME']=str(root/'agent');env['CODEWHALE_TELEMETRY']='0'
 requests=[{'jsonrpc':'2.0','id':n,'method':m,'params':{}} for n,m in enumerate(['healthz','capabilities','thread/start','shutdown'],1)]
 result=subprocess.run([str(binary),'app-server','--stdio','--config',str(config)],input=''.join(json.dumps(v)+'\n' for v in requests),text=True,capture_output=True,env=env,cwd=root,timeout=40)
 assert result.returncode==0,result.stderr[-1000:]
 replies={v['id']:v for line in result.stdout.splitlines() if (v:=json.loads(line)).get('id')}
 assert replies[1]['result']['version']=='0.9.13',replies[1]
 assert 'desktop/approval' in replies[2]['result']['methods']
 assert 'desktop/user-input' in replies[2]['result']['methods']
 assert 'desktop/session' in replies[2]['result']['methods']
 assert 'thread/steer' in replies[2]['result']['methods']
 assert replies[3]['result']['thread_id'],replies[3]
 print('PASS: sidecar version, desktop capabilities, thread/start, shutdown; no credentials or provider calls')
