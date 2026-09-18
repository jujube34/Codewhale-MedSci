"""Explicit local OCR, preserving page, boxes, confidence and reading order."""
import argparse,json,pathlib
import numpy as np
from PIL import Image
from rapidocr import RapidOCR
import onnxruntime

def main():
 p=argparse.ArgumentParser();p.add_argument('input',type=pathlib.Path);p.add_argument('output',type=pathlib.Path);a=p.parse_args()
 if 'CUDAExecutionProvider' in onnxruntime.get_available_providers():raise RuntimeError('仅允许 CPU OCR 运行环境')
 # Model files are installed from the hash-verified wheelhouse; reject a wheel
 # that lacks bundled models instead of allowing an implicit network download.
 import rapidocr
 models=list(pathlib.Path(rapidocr.__file__).parent.rglob('*.onnx'))
 if len(models)<3:raise RuntimeError('离线 OCR 模型不完整；请修复安装包，禁止运行时下载')
 params={'Global.use_cls':True,'EngineConfig.onnxruntime.use_cuda':False,
         'EngineConfig.onnxruntime.use_dml':False,'EngineConfig.onnxruntime.use_coreml':False,
         'EngineConfig.onnxruntime.use_cann':False,'EngineConfig.onnxruntime.intra_op_num_threads':2,
         'EngineConfig.onnxruntime.inter_op_num_threads':2}
 for role,filename in [('Det','PP-OCRv6_det_small.onnx'),('Cls','ch_ppocr_mobile_v2.0_cls_mobile.onnx'),('Rec','PP-OCRv6_rec_small.onnx')]:
  path=pathlib.Path(rapidocr.__file__).parent/'models'/filename
  if not path.is_file():raise RuntimeError('缺少离线模型: '+filename)
  params[role+'.model_path']=str(path)
 import socket
 def offline(*args,**kwargs):raise RuntimeError('本地 OCR 禁止网络访问')
 socket.create_connection=offline
 socket.socket.connect=offline
 engine=RapidOCR(params=params)
 results=[]
 if a.input.suffix.lower()=='.pdf':
  import fitz
  doc=fitz.open(a.input)
  def pages():
   for page in doc:
    pix=page.get_pixmap(dpi=200,alpha=False);yield np.frombuffer(pix.samples,np.uint8).reshape(pix.height,pix.width,pix.n)
 else:
  def pages():yield np.array(Image.open(a.input).convert('RGB'))
 for number,img in enumerate(pages(),1):
  output=engine(img)
  rows=[]
  if output.txts:
   for order,(box,text,score) in enumerate(zip(output.boxes,output.txts,output.scores)):
    rows.append({'order':order,'text':text,'confidence':float(score),'box':np.asarray(box).tolist()})
  results.append({'page':number,'items':rows})
 a.output.write_text(json.dumps({'source':'local_rapidocr_cpu','pages':results},ensure_ascii=False,indent=2),encoding='utf-8')
if __name__=='__main__':main()
