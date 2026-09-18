import pathlib,tempfile
from docx import Document
from pptx import Presentation
from openpyxl import Workbook,load_workbook
import fitz
with tempfile.TemporaryDirectory() as temp:
 d=pathlib.Path(temp)
 doc=Document();doc.add_paragraph('医学研究 · Preview');doc.save(d/'test.docx');doc=Document(d/'test.docx');assert '医学研究' in doc.paragraphs[0].text;doc.add_paragraph('修改副本');doc.save(d/'edited.docx')
 slides=Presentation();slides.slides.add_slide(slides.slide_layouts[1]).shapes.title.text='研究结果';slides.save(d/'test.pptx');slides=Presentation(d/'test.pptx');assert slides.slides[0].shapes.title.text=='研究结果';slides.save(d/'edited.pptx')
 book=Workbook();book.active.append(['结局',42]);book.save(d/'test.xlsx');book=load_workbook(d/'test.xlsx');assert book.active['B1'].value==42;book.active['B1']=43;book.save(d/'edited.xlsx')
 pdf=fitz.open();pdf.new_page().insert_text((72,72),'MedSci Preview');pdf.save(d/'test.pdf');pdf.close();pdf=fitz.open(d/'test.pdf');assert 'MedSci' in pdf[0].get_text();pdf[0].insert_text((72,100),'Edited copy');pdf.save(d/'edited.pdf');pdf.close()
 print('PASS: DOCX, PPTX, XLSX, PDF create/read/modify')
