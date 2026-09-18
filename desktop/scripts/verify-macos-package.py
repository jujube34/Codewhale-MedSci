"""Verify the delivered disk image and its immutable packaged payload."""
import hashlib,json,pathlib,subprocess,tempfile
root=pathlib.Path(__file__).resolve().parents[2]
dmg=root/'desktop/artifacts/Codewhale-MedSci_0.1.0-preview.7_arm64_internal.dmg'
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
assert digest(dmg)==dmg.with_suffix('.dmg.sha256').read_text().split()[0]
subprocess.run(['hdiutil','verify',str(dmg)],check=True,stdout=subprocess.DEVNULL)
with tempfile.TemporaryDirectory(prefix='medsci-dmg-') as tmp:
 mount=pathlib.Path(tmp)/'mount';mount.mkdir()
 subprocess.run(['hdiutil','attach','-readonly','-nobrowse','-mountpoint',str(mount),str(dmg)],check=True,stdout=subprocess.DEVNULL)
 try:
  app=mount/'Codewhale-MedSci.app';r=app/'Contents/Resources/resources'
  subprocess.run(['codesign','--verify','--deep','--strict',str(app)],check=True)
  p=json.loads((r/'build-provenance.json').read_text())
  # Signing changes the main executable. Its source-build hash is explicitly
  # labelled pre-signing; codesign verifies the final signed executable.
  assert digest(root/'apps/desktop/target/release/medsci-desktop')==p['desktop_before_signing_sha256']
  assert digest(r/'codewhale')==p['sidecar_sha256']
  assert digest(r/'source-manifest.json')==p['source_manifest_sha256']
  for w in json.loads((r/'python/wheelhouse-manifest.json').read_text())['wheels']:
   assert digest(r/'python/wheelhouse'/w['name'])==w['sha256'],w['name']
  subprocess.run(['python3',str(root/'desktop/scripts/protocol-smoke.py'),str(r/'codewhale')],check=True)
  print('PASS: DMG checksum/integrity, read-only mount, deep code signature, build provenance and all wheel hashes')
 finally:subprocess.run(['hdiutil','detach',str(mount)],check=True,stdout=subprocess.DEVNULL)
