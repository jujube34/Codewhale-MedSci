"""Exercise the real sidecar -> Runtime -> model codec -> stdio path offline."""
import http.server,json,os,pathlib,queue,subprocess,sys,tempfile,threading,time
INTERRUPT = "--interrupt" in sys.argv
STEER = "--steer" in sys.argv
REASONING = "--reasoning" in sys.argv
APPROVAL = "--approval" in sys.argv
NATIVE_SESSIONS = "--native-sessions" in sys.argv
SESSIONS = "--sessions" in sys.argv or NATIVE_SESSIONS
FULL_ACCESS = "--full-access" in sys.argv or SESSIONS or INTERRUPT
class Provider(http.server.BaseHTTPRequestHandler):
    calls=0
    tool_results=[]
    restored_history=False
    steer_seen=False
    paired_history=False
    fixture_path="must-not-exist.txt"
    def log_message(self,*args):pass
    def do_GET(self):
        self.send_response(200);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(json.dumps({'data':[{'id':'deepseek-flash'}]}).encode())
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        Provider.calls+=1
        if STEER:
            Provider.steer_seen |= any('STEER_NATIVE_FIXTURE' in str(m.get('content','')) for m in body.get('messages',[]) if m.get('role')=='user')
        if Provider.calls>=3:
            Provider.restored_history=any(m.get('role')=='assistant' and '本地协议验证成功' in str(m.get('content','')) for m in body.get('messages',[]))
        Provider.tool_results=[m for m in body.get("messages",[]) if m.get("role")=="tool"]
        if INTERRUPT and Provider.calls>1:
            calls={t['id'] for m in body.get('messages',[]) for t in m.get('tool_calls',[])}
            results={m.get('tool_call_id') for m in body.get('messages',[]) if m.get('role')=='tool'}
            Provider.paired_history=bool(calls) and calls.issubset(results)
            if not Provider.paired_history:
                self.send_response(400);self.end_headers();self.wfile.write(b'{"error":{"message":"No tool output found for tool call"}}');return
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
        if STEER and Provider.calls==1:
            frame={'id':'fixture','choices':[{'index':0,'delta':{'reasoning_content':'正在检查资料'},'finish_reason':None}]}
            self.wfile.write(('data: '+json.dumps(frame)+'\n\n').encode());self.wfile.flush();time.sleep(2)
        if (APPROVAL or FULL_ACCESS) and Provider.calls==1:
            names=[t['function']['name'] for t in body.get('tools',[]) if 'function' in t]
            tool='File' if 'File' in names else 'write_file'
            args={'path':Provider.fixture_path,'content':'must remain absent'}
            if tool=='File':args['action']='write'
            if INTERRUPT:
                tool='bash';args={'command':'sleep 30'}
            frame={'id':'fixture','choices':[{'index':0,'delta':{'tool_calls':[{'index':0,'id':'call_fixture','type':'function','function':{'name':tool,'arguments':json.dumps(args)}}]},'finish_reason':'tool_calls'}]}
            self.wfile.write(('data: '+json.dumps(frame)+'\n\ndata: [DONE]\n\n').encode());self.wfile.flush();return
        if REASONING:
            for word in ['先检查', '本地资料。']:
                frame={'id':'fixture','choices':[{'index':0,'delta':{'reasoning_content':word},'finish_reason':None}]}
                self.wfile.write(('data: '+json.dumps(frame)+'\n\n').encode());self.wfile.flush();time.sleep(.1)
        for word in ['本地' ,'协议','验证成功']:
            frame={'id':'fixture','object':'chat.completion.chunk','model':'deepseek-flash','choices':[{'index':0,'delta':{'content':word},'finish_reason':None}]}
            self.wfile.write(('data: '+json.dumps(frame)+'\n\n').encode())
        self.wfile.write(b'data: {"id":"fixture","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n');self.wfile.flush()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider);threading.Thread(target=server.serve_forever,daemon=True).start()
with tempfile.TemporaryDirectory(prefix='medsci-conversation-') as temp:
    root=pathlib.Path(temp);config=root/'config.toml'
    workspace=root/'workspace' if FULL_ACCESS else root
    workspace.mkdir(exist_ok=True)
    if FULL_ACCESS:Provider.fixture_path=str(root/'must-not-exist.txt')
    config.write_text(f'provider="deepseek"\nmodel="deepseek-flash"\nbase_url="http://127.0.0.1:{server.server_port}/v1"\napproval_policy="untrusted"\nsandbox_mode="workspace-write"\ntelemetry=false\n[features]\nmcp=false\n')
    if FULL_ACCESS:
        config.write_text(config.read_text().replace('approval_policy="untrusted"','approval_policy="auto"').replace('sandbox_mode="workspace-write"','sandbox_mode="danger-full-access"'))
    env={k:v for k,v in os.environ.items() if k.upper() in ['PATH','HOME','SYSTEMROOT','WINDIR','TEMP','TMP','USERPROFILE','LOCALAPPDATA','APPDATA']};env.update(CODEWHALE_HOME=str(root/'agent'),CODEWHALE_SESSION_ID='mock-native-session',CODEWHALE_TELEMETRY='0',DEEPSEEK_API_KEY='fixture-not-a-real-key')
    proc=subprocess.Popen([str(pathlib.Path(sys.argv[1]).resolve()),'app-server','--stdio','--config',str(config)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,encoding='utf-8',env=env,cwd=workspace)
    out=queue.Queue()
    def reader():
        for line in proc.stdout:out.put(json.loads(line))
    threading.Thread(target=reader,daemon=True).start()
    def send(id,method,params):proc.stdin.write(json.dumps({'jsonrpc':'2.0','id':id,'method':method,'params':params})+'\n');proc.stdin.flush()
    try:
        send(1,'thread/start',{'cwd':str(workspace),'model':'deepseek-flash','model_provider':'deepseek'})
        start=out.get(timeout=30);assert 'error' not in start,start
        send(90,'desktop/session',{'thread_id':start['result']['thread_id'],'operation':'new'})
        initialized=out.get(timeout=30);assert 'error' not in initialized,initialized
        send(2,'thread/message',{'thread_id':start['result']['thread_id'],'input':'请只回答本地协议验证成功'})
        text='';interrupt_sent=False;steer_sent=False;steer_accepted=False;reasoning='';reasoning_before_response=False;approval_seen=False;deadline=time.monotonic()+60
        while time.monotonic()<deadline:
            event=out.get(timeout=60)
            if event.get('event')=='item.delta' and event.get('payload',{}).get('kind')=='agent_reasoning':
                reasoning+=event['payload']['delta'];reasoning_before_response=not text
            if event.get('type')=='response_delta':text+=event['delta']
            if INTERRUPT and not interrupt_sent and event.get('event')=='item.started' and event.get('payload',{}).get('tool',{}).get('name')=='bash':
                time.sleep(.3)
                send(12,'thread/interrupt',{'thread_id':start['result']['thread_id']});interrupt_sent=True
            if STEER and not steer_sent and event.get('event')=='item.delta' and event.get('payload',{}).get('kind')=='agent_reasoning':
                send(11,'thread/steer',{'thread_id':start['result']['thread_id'],'prompt':'STEER_NATIVE_FIXTURE：请根据这条补充指令继续当前任务'})
                steer_sent=True
            if event.get('event')=='turn.steered':steer_accepted=True
            if event.get('event')=='approval.required':
                approval_seen=True;p=event['payload'];cap=p.get('approval_id') or p['id']
                send(9,'desktop/approval',{'thread_id':start['result']['thread_id'],'id':cap,'decision':'deny'})
            if event.get('id')==2:
                if not INTERRUPT:assert 'error' not in event,event
                else:assert 'error' not in event or event['error']['message']=='turn interrupted',event
                break
        if INTERRUPT:
            assert interrupt_sent,'shell interruption did not run'
            def reply(id):
                while True:
                    event=out.get(timeout=60)
                    if event.get('id')==id:
                        assert 'error' not in event,event
                        return event['result']
            send(30,'desktop/session',{'thread_id':start['result']['thread_id'],'operation':'save'})
            summary=reply(30);runtime_id=summary['runtime_id']
            item=next(i for i in summary['detail']['items'] if i.get('metadata',{}).get('tool_use_id')=='call_fixture')
            assert item['status']=='failed' and 'interrupted after shell work started' in item.get('detail',''), item
            assert item['metadata'].get('tool_result_for')=='call_fixture', 'interrupted tool must persist its terminal result'
            send(31,'thread/message',{'thread_id':start['result']['thread_id'],'input':'继续刚才的任务'})
            reply(31);assert Provider.paired_history
            send(32,'shutdown',{});proc.wait(timeout=10)
            proc=subprocess.Popen([str(pathlib.Path(sys.argv[1]).resolve()),'app-server','--stdio','--config',str(config)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,encoding='utf-8',env=env,cwd=workspace)
            out=queue.Queue();threading.Thread(target=reader,daemon=True).start()
            send(33,'thread/start',{'cwd':str(workspace),'model':'deepseek-flash','model_provider':'deepseek'});resumed=reply(33)
            send(34,'desktop/session',{'thread_id':resumed['thread_id'],'session_id':'mock-native-session'});assert reply(34)['runtime_id']==runtime_id
            send(35,'thread/message',{'thread_id':resumed['thread_id'],'input':'重启后继续同一个任务'})
            reply(35);assert Provider.paired_history
            print('PASS: interrupted live bash persists paired tool result; follow-up works in same process and after native session restart; strict provider rejects unpaired calls')
            send(36,'shutdown',{});proc.wait(timeout=10)
            raise SystemExit(0)
        if REASONING:
            assert reasoning=='先检查本地资料。' and reasoning_before_response,(reasoning,text)
            print('PASS: provider reasoning_content streams through real Runtime before final response; both remain distinct')
        assert text.endswith('本地协议验证成功') if STEER else text=='本地协议验证成功',(text,Provider.calls)
        if not STEER:assert Provider.calls==(2 if (APPROVAL or FULL_ACCESS) else 1),Provider.calls
        if STEER:
            assert steer_sent and steer_accepted and Provider.steer_seen,(steer_sent,steer_accepted,Provider.steer_seen,Provider.calls)
            send(50,'desktop/session',{'thread_id':start['result']['thread_id']})
            while True:
                record=out.get(timeout=30)
                if record.get('id')==11:assert 'error' not in record,record
                if record.get('id')==50:break
            detail=record['result']['detail']
            assert len(detail['turns'])==1 and detail['turns'][0]['steer_count']==1,detail
            assert any('STEER_NATIVE_FIXTURE' in i.get('detail','') for i in detail['items']),detail
            print('PASS: native steer accepted mid-stream, reached the model in the same turn, and persisted in native history; no extra turn/session')
        if APPROVAL:assert approval_seen and not (root/'must-not-exist.txt').exists(), 'denied tool must not write' 
        if FULL_ACCESS:assert not approval_seen and (root/'must-not-exist.txt').exists() and (root/'must-not-exist.txt').read_text()=='must remain absent', Provider.tool_results
        if SESSIONS:
            send(30,'desktop/session',{'thread_id':start['result']['thread_id'],'operation':'save'})
            summary=out.get(timeout=30);assert 'error' not in summary,summary
            runtime_id=summary['result']['runtime_id']
            assert summary['result']['usage']['context']['used_tokens']>0,summary
            if NATIVE_SESSIONS:
                listed=json.loads(subprocess.check_output([str(pathlib.Path(sys.argv[1]).resolve()),'sessions','list','--json','--workspace',str(workspace)],env=env,text=True))
                assert [row['id'] for row in listed]==['mock-native-session'],listed
                assert summary['result']['session_id']=='mock-native-session'
            send(31,'shutdown',{});proc.wait(timeout=10)
            proc=subprocess.Popen([str(pathlib.Path(sys.argv[1]).resolve()),'app-server','--stdio','--config',str(config)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,encoding='utf-8',env=env,cwd=workspace)
            out=queue.Queue();threading.Thread(target=reader,daemon=True).start()
            send(32,'thread/start',{'cwd':str(workspace),'model':'deepseek-flash','model_provider':'deepseek'})
            resumed=out.get(timeout=30);assert 'error' not in resumed,resumed
            send(33,'desktop/session',{'thread_id':resumed['result']['thread_id'],'session_id':'mock-native-session'})
            record=out.get(timeout=30);assert record.get('result',{}).get('runtime_id')==runtime_id,record
            send(34,'thread/message',{'thread_id':resumed['result']['thread_id'],'input':'继续之前的会话'})
            while True:
                event=out.get(timeout=60)
                if event.get('id')==34:
                    assert 'error' not in event,event
                    break
            assert Provider.restored_history,'runtime must restore the prior assistant message across process restart'
            if NATIVE_SESSIONS:
                assert any(i.get('metadata',{}).get('tool_name') for i in record['result']['detail']['items']), 'native history must retain tool calls'
            other=root/'other';other.mkdir()
            send(35,'thread/start',{'cwd':str(other),'model':'deepseek-flash','model_provider':'deepseek'})
            other_start=out.get(timeout=30)
            send(36,'desktop/session',{'thread_id':other_start['result']['thread_id'],'session_id':'mock-native-session'})
            denied=out.get(timeout=30);assert 'error' in denied,denied
            print('PASS: native SavedSession restores tool history and model context after restart; cross-workspace selection is rejected')
        send(3,'shutdown',{});proc.wait(timeout=10)
        print('PASS: '+('full access executed an out-of-workspace fixture write without an approval prompt' if FULL_ACCESS else 'native mid-turn steer round-trip completed' if STEER else 'mid-turn approval denial prevented the file write' if APPROVAL else 'real sidecar streamed Chinese text through one mock provider request')+'; no real API key or vendor calls')
    finally:
        proc.kill();proc.wait();server.shutdown()
