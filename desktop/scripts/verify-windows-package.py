"""Check the assembled NSIS payload without claiming native Windows GUI testing."""
import argparse,hashlib,json,pathlib,struct,subprocess,tempfile
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--version',default='0.1.0-preview.7.5')
parser.add_argument('--stage',type=pathlib.Path)
args=parser.parse_args()
root=pathlib.Path(__file__).resolve().parents[2]
artifacts=root/'desktop/artifacts'
installer=artifacts/f'Codewhale-MedSci_{args.version}_windows-x64_internal-setup.exe'
stage=args.stage or artifacts/f'stage-windows-x86_64-{args.version}'
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
assert digest(installer)==installer.with_suffix('.exe.sha256').read_text().split()[0]
result=subprocess.run(['7z','t',str(installer)],capture_output=True,text=True,check=True)
assert 'Everything is Ok' in result.stdout,result.stdout
with tempfile.TemporaryDirectory(prefix='windows-payload-',dir=artifacts) as temporary:
    extracted=pathlib.Path(temporary)
    subprocess.run(['7z','x','-y',f'-o{extracted}',str(installer)],stdout=subprocess.DEVNULL,check=True)
    init_scripts=list(extracted.rglob('medsci-git-init.cmd'))
    assert len(init_scripts)==1,init_scripts
    assert digest(init_scripts[0])==digest(root/'desktop/packaging/windows-git-init.cmd')
    candidates=list(extracted.rglob('medsci-desktop.exe'))
    assert len(candidates)==1,candidates
    payload=candidates[0].parent
    count=0
    for path in stage.rglob('*'):
        if path.is_file():
            relative=path.relative_to(stage)
            actual=payload/relative
            assert actual.is_file() and digest(actual)==digest(path),relative
            count+=1
    for relative in ['medsci-desktop.exe','resources/codewhale.exe','resources/python/cpython/python.exe']:
        data=(payload/relative).read_bytes();offset=struct.unpack_from('<I',data,0x3c)[0]
        assert data[:2]==b'MZ' and data[offset:offset+4]==b'PE\0\0'
        assert struct.unpack_from('<H',data,offset+4)[0]==0x8664,relative
    resources=payload/'resources'
    provenance=json.loads((resources/'build-provenance.json').read_text())
    assert digest(resources/'codewhale.exe')==provenance['sidecar_sha256']
    assert digest(payload/'medsci-desktop.exe')==provenance['desktop_sha256']
    assert digest(resources/'source-manifest.json')==provenance['source_manifest_sha256']
    git=resources/'git-bash'
    for name,sha in json.loads((git/'medsci-git-manifest.json').read_text())['files'].items():
        assert digest(git/name)==sha,name
    for wheel in json.loads((resources/'python/wheelhouse-manifest.json').read_text())['wheels']:
        assert digest(resources/'python/wheelhouse'/wheel['name'])==wheel['sha256'],wheel['name']
    for dll,sha in json.loads((resources/'python/vc-runtime/manifest.json').read_text())['files'].items():
        assert digest(resources/'python/vc-runtime'/dll)==sha
        assert digest(payload/dll)==digest(resources/'python/cpython'/dll)==digest(resources/dll)==sha
    print(f'PASS: NSIS integrity, SHA-256, {count} extracted payload files match staging, x64 PE binaries, VC runtime DLLs, source provenance, and all offline wheel hashes')
print('This check does not exercise native installation/uninstallation, WebView2 or GUI behavior.')
