"""Encoding and raw-error preservation at the actual runtime subprocess boundary."""
import importlib.util,sys,tempfile,pathlib
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('manager',pathlib.Path(__file__).resolve().parents[1]/'runtime/manager.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
for encoding,text in [('utf-8','无法加载 DLL：中文路径'),('gbk','无法加载 DLL：中文路径'),('cp1252','DLL café introuvable')]:
    with patch.object(m.locale,'getencoding',return_value=encoding):
        assert m.decode_output(text.encode(encoding))==text
        result=m.run([sys.executable,'-I','-c',f'import sys;sys.stdout.buffer.write({text.encode(encoding)!r})'])
        assert result==text
    print('PASS:',encoding,'subprocess output')
with tempfile.TemporaryDirectory() as temp:
    m.LOG_DIR=pathlib.Path(temp)
    raw='找不到指定模块'.encode('gbk')
    with patch.object(m.locale,'getencoding',return_value='cp936'):
        try:m.run([sys.executable,'-I','-c',f'import sys;sys.stderr.buffer.write({raw!r});sys.exit(3)'])
        except RuntimeError as error:assert '找不到指定模块' in str(error) and '3' in str(error)
        else:raise AssertionError('failure swallowed')
    assert (m.LOG_DIR/'subprocess-stderr.bin').read_bytes()==raw
    print('PASS: exact raw error bytes preserved with readable GBK message and exit code')
assert m.run([sys.executable,'-I','-c','import sys;print(sys.stdout.encoding)']).strip()=='utf-8'
print('PASS: isolated Python subprocess forced to UTF-8')
print('5 checks passed; 0 failed')
