"""Real _extra import: original CPython lacks MSVCP, packaged CPython supplies it."""
import os,pathlib,subprocess,tempfile,zipfile
root=pathlib.Path(__file__).resolve().parents[2]
def win(p):return 'Z:'+str(p.resolve()).replace('/','\\')
env={k:v for k,v in os.environ.items() if k in ['PATH','HOME','TMPDIR','USER','LOGNAME']}
env.update(WINEPREFIX=str(root/'desktop/test-wine'),WINEDEBUG='-all',WINEDLLOVERRIDES='msvcp140=n')
with tempfile.TemporaryDirectory(prefix='dll-regression-',dir=root/'desktop/artifacts') as temp:
    folder=pathlib.Path(temp)
    with zipfile.ZipFile(next((root/'desktop/runtime/windows-x86_64/wheelhouse').glob('pymupdf-*.whl'))) as archive:
        for name in archive.namelist():
            if name.endswith(('.pyd','.dll')):(folder/pathlib.Path(name).name).write_bytes(archive.read(name))
    script=folder/'probe.py';script.write_text(f'import sys\nsys.path.insert(0,{win(folder)!r})\nimport _extra\nprint("IMPORT PASSED")\n')
    for name,base in [('before',root/'desktop/runtime/cpython/windows-payload/python'),('after',root/'desktop/artifacts/stage-windows-x86_64/resources/python/cpython')]:
        log=folder/(name+'.log');batch=folder/'run.cmd'
        batch.write_text('@echo off\n"'+win(base/'python.exe')+'" -I -X utf8 "'+win(script)+'" > "'+win(log)+'" 2>&1\nexit /b %ERRORLEVEL%\n')
        result=subprocess.run(['wine','cmd','/d','/c',win(batch)],env=env,timeout=60)
        output=log.read_text(encoding='utf-8',errors='replace')
        if name=='before':assert result.returncode!=0 and 'ImportError: DLL load failed while importing _extra' in output,output
        else:assert result.returncode==0 and 'IMPORT PASSED' in output,output
        print('PASS:',name,output.strip(),flush=True)
print('2 checks passed; 0 failed. Wine builtin MSVCP disabled; actual PyMuPDF extension used.')
