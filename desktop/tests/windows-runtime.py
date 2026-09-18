"""Run actual Windows offline initialization, imports and document round trips in Wine."""
import os,pathlib,subprocess,tempfile,sys
root=pathlib.Path(__file__).resolve().parents[2]
runtime=root/'desktop/artifacts/stage-windows-x86_64/resources/python'
def win(p):return 'Z:'+str(p.resolve()).replace('/','\\')
with tempfile.TemporaryDirectory(prefix='windows-runtime-',dir=root/'desktop/artifacts') as temporary:
    folder=pathlib.Path(temporary);driver=folder/'driver.py'
    driver.write_text('import subprocess,sys,pathlib\n'+
        f'runtime=pathlib.Path({win(runtime)!r})\nhome=pathlib.Path({win(folder)!r})/"用户 shared"\n'+
        'for command in ["initialize","check","initialize"]:\n'+
        ' subprocess.run([sys.executable,"-I","-X","utf8",str(runtime/"manager.py"),"--home",str(home),command],check=True)\n'+
        ' print("PASS: shared runtime "+command,flush=True)\n'+
        (f'subprocess.run([str(home/"current/Scripts/python.exe"),"-I","-X","utf8",{win(root/"desktop/tests/document_smoke.py")!r}],check=True)\n' if '--documents' in sys.argv else ''),encoding='utf-8')
    batch=folder/'run.cmd'
    batch.write_text('@echo off\n"'+win(runtime/'cpython/python.exe')+'" -I -X utf8 "'+win(driver)+'" > "'+win(folder/'output.log')+'" 2>&1\nexit /b %ERRORLEVEL%\n')
    env={k:v for k,v in os.environ.items() if k in ['PATH','HOME','TMPDIR','USER','LOGNAME']}
    env.update(WINEPREFIX=str(root/'desktop/test-wine'),WINEDEBUG='-all',WINEDLLOVERRIDES='msvcp140,msvcp140_1,msvcp140_2,msvcp140_atomic_wait,msvcp140_codecvt_ids,concrt140=n')
    result=subprocess.run(['wine','cmd','/d','/c',win(batch)],env=env,timeout=300)
    print((folder/'output.log').read_text(errors='replace'),flush=True)
    assert result.returncode==0,result.returncode
    print('PASS under Wine: fresh offline Windows environment, all core imports/CPU ONNX, pip check, published environment recheck/reuse, Unicode/spaced path. No real model calls.')
