import email,hashlib,json,pathlib,zipfile,sys
root=pathlib.Path(sys.argv[1]) if len(sys.argv)>1 else pathlib.Path(__file__).resolve().parents[1]/'runtime'
rows=[]; requirements=[]
for path in sorted((root/'wheelhouse').glob('*.whl')):
 with zipfile.ZipFile(path) as z:
  name=next(n for n in z.namelist() if n.endswith('.dist-info/METADATA'))
  metadata=email.message_from_bytes(z.read(name))
 sha=hashlib.sha256(path.read_bytes()).hexdigest()
 rows.append({'name':path.name,'sha256':sha,'distribution':metadata['Name'],'version':metadata['Version'],'license':str(metadata.get('License-Expression') or metadata.get('License') or 'REVIEW REQUIRED'),'source':'https://pypi.org/project/'+metadata['Name']+'/'+metadata['Version']+'/'})
 requirements.append(f"{metadata['Name']}=={metadata['Version']} --hash=sha256:{sha}")
(root/'requirements.lock').write_text('\n'.join(requirements)+'\n')
(root/'wheelhouse-manifest.json').write_text(json.dumps({'schema_version':1,'wheels':rows},indent=2)+'\n')
print(f'Locked {len(rows)} wheels')
