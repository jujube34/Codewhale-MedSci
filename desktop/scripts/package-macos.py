"""Assemble an internal ad-hoc signed DMG from built native artifacts.
No Developer ID signing or notarization is claimed by this script.
"""
import hashlib,json,pathlib,plistlib,shutil,subprocess,sys
root=pathlib.Path(__file__).resolve().parents[2];arch=sys.argv[1] if len(sys.argv)>1 else 'arm64'
triple={'arm64':'aarch64-apple-darwin','x86_64':'x86_64-apple-darwin'}[arch]
artifacts=root/'desktop/artifacts';artifacts.mkdir(exist_ok=True)
stage=artifacts/f'stage-macos-{arch}';stage.mkdir(exist_ok=True)
app=stage/'Codewhale-MedSci.app'
template=root/'apps/desktop/target/release/bundle/macos/Codewhale-MedSci.app'
if app.exists():shutil.rmtree(app)
shutil.copytree(template,app,symlinks=True)
info_path=app/'Contents/Info.plist'
info=plistlib.loads(info_path.read_bytes())
info['CFBundleShortVersionString']='0.1.0'
info['CFBundleVersion']='7'
info_path.write_bytes(plistlib.dumps(info))

bin_dir=root/'apps/desktop/target'/('release' if arch=='arm64' else f'{triple}/release')
shutil.copy2(bin_dir/'medsci-desktop',app/'Contents/MacOS/medsci-desktop')
resources=app/'Contents/Resources/resources';resources.mkdir(exist_ok=True)
agent=root/'target'/('release' if arch=='arm64' else f'{triple}/release')/'codewhale'
shutil.copy2(agent,resources/'codewhale')
runtime=root/'desktop/runtime';source=runtime if arch=='arm64' else runtime/'macos-x86_64'
payload=resources/'python'
if payload.exists():shutil.rmtree(payload)
payload.mkdir()
for name in ['manager.py','ocr.py','ocr-models-manifest.json']:shutil.copy2(runtime/name,payload/name)
for name in ['requirements.lock','wheelhouse-manifest.json']:shutil.copy2(source/name,payload/name)
shutil.copytree(source/'wheelhouse',payload/'wheelhouse')
shutil.copytree(runtime/f'cpython/cpython-3.12.13-macos-{ "aarch64" if arch=="arm64" else "x86_64"}-none',payload/'cpython',symlinks=True)
ext=app/'Contents/PlugIns/MedSciFinder.appex';(ext/'Contents/MacOS').mkdir(parents=True,exist_ok=True)
shutil.copy2(root/'desktop/packaging/finder/Info.plist',ext/'Contents/Info.plist')
subprocess.run(['swiftc','-emit-executable','-parse-as-library','-application-extension','-module-name','FinderExtension','-target',f'{arch}-apple-macos13.0','-framework','Cocoa','-framework','FinderSync','-Xlinker','-e','-Xlinker','_NSExtensionMain',str(root/'desktop/packaging/finder/FinderSync.swift'),'-o',str(ext/'Contents/MacOS/FinderExtension')],check=True)
shutil.copy2(root/'desktop/THIRD_PARTY_NOTICES.md',resources/'THIRD_PARTY_NOTICES.md')
provenance={'schema_version':1,'upstream_commit':'e9b761c08accec7f2ca2a7202a5d710b72faf325','source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'source_diff_sha256':hashlib.sha256(subprocess.check_output(['git','diff','HEAD','--binary'],cwd=root)).hexdigest(),'target':triple,'channel':'internal-development','signature':'ad-hoc','notarized':False,'sidecar_sha256':hashlib.sha256(agent.read_bytes()).hexdigest()}
# Include uncommitted and untracked source files in build provenance.
source_paths=subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard'],cwd=root).decode().split('\0')
source_hashes={name:hashlib.sha256((root/name).read_bytes()).hexdigest() for name in sorted(set(source_paths)) if name and (root/name).is_file()}
(resources/'source-manifest.json').write_text(json.dumps(source_hashes,indent=2,ensure_ascii=False)+'\n')
provenance['source_manifest_sha256']=hashlib.sha256((resources/'source-manifest.json').read_bytes()).hexdigest()
provenance['desktop_before_signing_sha256']=hashlib.sha256((bin_dir/'medsci-desktop').read_bytes()).hexdigest()
(resources/'build-provenance.json').write_text(json.dumps(provenance,indent=2)+'\n')
subprocess.run(['codesign','--force','--sign','-','--entitlements',str(root/'desktop/packaging/finder/entitlements.plist'),str(ext)],check=True)
subprocess.run(['codesign','--force','--deep','--sign','-',str(app)],check=True)
subprocess.run(['codesign','--verify','--deep','--strict',str(app)],check=True)
link=stage/'Applications'
if not link.exists():link.symlink_to('/Applications')
shutil.copy2(root/'desktop/README.md',stage/'阅读说明.md')
output=artifacts/f'Codewhale-MedSci_0.1.0-preview.7_{arch}_internal.dmg'
subprocess.run(['hdiutil','create','-volname','Codewhale-MedSci Preview','-srcfolder',str(stage),'-ov','-format','UDZO',str(output)],check=True)
(output.with_suffix('.dmg.sha256')).write_text(hashlib.sha256(output.read_bytes()).hexdigest()+'  '+output.name+'\n')
print(output)
