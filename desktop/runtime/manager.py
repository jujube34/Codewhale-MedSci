"""Host-only shared Python transactions. No project-specific environments.

The desktop invokes this interpreter with -I. Install is a user action while
Agent is stopped; this CLI is never exposed as an unapproved Agent tool.
"""
import argparse, contextlib, hashlib, importlib.metadata, json, locale, os, pathlib, shutil, subprocess, sys, tempfile, time
ROOT = pathlib.Path(__file__).resolve().parent
CORE = ['docx','pptx','pandas','openpyxl','fitz','pypdf','reportlab','PIL','matplotlib','numpy','bs4','lxml','rapidocr','onnxruntime']
def interpreter(env): return env / ('Scripts/python.exe' if os.name == 'nt' else 'bin/python3')
def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda:f.read(1024*1024),b''): h.update(chunk)
    return h.hexdigest()
def clean_env():
    e={k:v for k,v in os.environ.items() if not k.upper().startswith(('PIP_','PYTHON','HTTP_PROXY','HTTPS_PROXY','ALL_PROXY'))}
    e.update(PIP_CONFIG_FILE=os.devnull,PYTHONNOUSERSITE='1',PYTHONDONTWRITEBYTECODE='1',PIP_DISABLE_PIP_VERSION_CHECK='1')
    return e
LOG_DIR = None
def decode_output(data):
    # Internal Python is UTF-8. Legacy native output follows the Windows ACP.
    for encoding in dict.fromkeys(['utf-8-sig', locale.getencoding(), 'gb18030']):
        try: return data.decode(encoding)
        except (UnicodeError, LookupError): pass
    return data.decode('utf-8', errors='backslashreplace')
def run(args):
    args=[str(a) for a in args]
    # All callers run Python; -I ignores PYTHONUTF8, so use the explicit flag.
    args=[args[0],'-X','utf8',*args[1:]]
    result=subprocess.run(args,env=clean_env(),capture_output=True)
    if result.returncode:
        if LOG_DIR is not None:
            LOG_DIR.mkdir(parents=True,exist_ok=True)
            (LOG_DIR/'subprocess-stderr.bin').write_bytes(result.stderr)
            (LOG_DIR/'subprocess-stdout.bin').write_bytes(result.stdout)
            (LOG_DIR/'subprocess.json').write_text(json.dumps({'args':args,'exit_code':result.returncode,'system_encoding':locale.getencoding()},ensure_ascii=False,indent=2),encoding='utf-8')
        detail=decode_output(result.stderr or result.stdout).strip()[-3000:]
        raise RuntimeError(f'Python 子进程退出码 {result.returncode}: {detail or "未返回错误详情"}')
    return decode_output(result.stdout)
def runtime_hash():
    manifest=ROOT/'vc-runtime/manifest.json'
    return digest(manifest) if os.name=='nt' and manifest.is_file() else None
def install_runtime_dlls(env):
    if os.name!='nt': return
    source=ROOT/'vc-runtime'
    manifest=json.loads((source/'manifest.json').read_text(encoding='utf-8'))
    for name,sha in manifest['files'].items():
        if digest(source/name)!=sha: raise RuntimeError('C++ runtime 校验失败: '+name)
        shutil.copyfile(source/name,env/'Scripts'/name)
@contextlib.contextmanager
def locked(home):
    home.mkdir(parents=True,exist_ok=True)
    with (home/'transaction.lock').open('a+b') as f:
        if os.name=='nt':
            import msvcrt
            f.seek(0);f.write(b'0');f.flush();f.seek(0);msvcrt.locking(f.fileno(),msvcrt.LK_NBLCK,1)
        else:
            import fcntl
            fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)
        yield

def smoke(python):
    run([python,'-I','-m','pip','check'])
    run([python,'-I','-c',"import importlib; [importlib.import_module(n) for n in "+repr(CORE)+"]; import onnxruntime as ort; assert 'CPUExecutionProvider' in ort.get_available_providers(); assert 'CUDAExecutionProvider' not in ort.get_available_providers()"])

def recover(home):
    current,backup=home/'current',home/'rollback'
    if backup.exists() and not current.exists(): backup.rename(current)
    elif backup.exists(): shutil.rmtree(backup)
    for p in home.glob('.stage-*'): shutil.rmtree(p)

def transact(home, package=None):
    global LOG_DIR
    LOG_DIR=home/'logs'
    with locked(home):
        recover(home)
        current=home/'current'
        previous={}
        if (current/'manifest.json').exists(): previous=json.loads((current/'manifest.json').read_text())
        compatible=previous.get('base_lock')==digest(ROOT/'requirements.lock') and previous.get('base_executable')==sys.executable and previous.get('python')==sys.version.split()[0] and previous.get('runtime_hash')==runtime_hash()
        if package is None and current.exists() and compatible: smoke(interpreter(current));return
        stage=pathlib.Path(tempfile.mkdtemp(prefix='.stage-',dir=home))
        try:
            run([sys.executable,'-I','-m','venv','--copies' if os.name=='nt' else '--symlinks',stage]);python=interpreter(stage)
            install_runtime_dlls(stage)
            manifest=json.loads((ROOT/'wheelhouse-manifest.json').read_text())
            for item in manifest['wheels']:
                path=ROOT/'wheelhouse'/item['name']
                if digest(path)!=item['sha256']:raise RuntimeError('离线 wheel 校验失败: '+item['name'])
            run([python,'-I','-m','pip','install','--no-index','--only-binary=:all:','--find-links',ROOT/'wheelhouse','--require-hashes','-r',ROOT/'requirements.lock'])
            requested=previous.get('requested',[])
            if package:
                name=package.split('==')[0].lower().replace('_','-')
                requested=[x for x in requested if x.split('==')[0].lower().replace('_','-')!=name]+[package]
            if requested:run([python,'-I','-m','pip','install','--only-binary=:all:',*requested])
            smoke(python)
            freeze=run([python,'-I','-m','pip','freeze']);(stage/'resolved.lock').write_text(freeze)
            (stage/'manifest.json').write_text(json.dumps({'schema_version':1,'runtime_hash':runtime_hash(),'python':sys.version.split()[0],'base_executable':sys.executable,'requested':requested,'base_lock':digest(ROOT/'requirements.lock'),'updated_at':time.time(),'packages':freeze.splitlines()},indent=2))
            # Entry-point scripts embed the build path. Rewrite before publishing.
            bindir=stage/('Scripts' if os.name=='nt' else 'bin')
            if os.name!='nt':
                for p in bindir.iterdir():
                    if p.is_file() and not p.is_symlink():
                        blob=p.read_bytes()
                        if blob.startswith(b'#!'):p.write_bytes(blob.replace(str(stage).encode(),str(current).encode()))
            backup=home/'rollback'
            if current.exists():current.rename(backup)
            try:stage.rename(current)
            except BaseException:
                if backup.exists():backup.rename(current)
                raise
            if backup.exists():shutil.rmtree(backup)
        finally:
            if stage.exists():shutil.rmtree(stage)

def main():
    for stream in (sys.stdout,sys.stderr):
        if hasattr(stream,'reconfigure'): stream.reconfigure(encoding='utf-8',errors='backslashreplace')
    parser=argparse.ArgumentParser();parser.add_argument('--home',required=True,type=pathlib.Path)
    parser.add_argument('command',choices=['initialize','check','inventory','install']);parser.add_argument('package',nargs='?');a=parser.parse_args()
    if a.command in ('initialize','install'):
        if a.command=='install':
            import re
            if not a.package or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*(?:==[A-Za-z0-9._+-]+)?',a.package):raise SystemExit('只接受包名或 name==version；禁止 URL、路径和 pip 选项')
        transact(a.home,a.package if a.command=='install' else None)
    elif a.command=='check':smoke(interpreter(a.home/'current'))
    else:
        print(run([interpreter(a.home/'current'),'-I','-c',"import importlib.metadata,json;print(json.dumps(sorted([{'name':d.metadata['Name'],'version':d.version} for d in importlib.metadata.distributions()],key=lambda d:d['name'])))"]))
if __name__=='__main__':main()
