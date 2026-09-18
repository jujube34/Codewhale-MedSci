import init from './pkg/medsci_ui.js';
window.medsciInvoke = async (command, args) => {
  if (!window.__TAURI__) throw new Error('请在已安装的桌面应用中使用此功能');
  return window.__TAURI__.core.invoke(command,args);
};
window.medsciNative = () => Boolean(window.__TAURI__);
document.documentElement.classList.toggle('windows', /Win/.test(navigator.platform));
if (document.documentElement.classList.contains('windows')) {
 const scrollTimers = new WeakMap();
 document.addEventListener('scroll', event => {
  const target = event.target === document ? document.scrollingElement : event.target;
  if (!(target instanceof Element)) return;
  target.classList.add('is-scrolling');
  clearTimeout(scrollTimers.get(target));
  scrollTimers.set(target, setTimeout(() => {
   target.classList.remove('is-scrolling');
   scrollTimers.delete(target);
  }, 700));
 }, {capture:true, passive:true});
}

let attachments = [];
window.medsciImages = () => attachments.map(({mime,dataBase64})=>({mime,dataBase64}));
window.medsciClearImages = () => {attachments=[];renderAttachments();};
function renderAttachments(){
 const host=document.getElementById('attachments'); if(!host)return;
 host.replaceChildren();
 attachments.forEach((a,i)=>{const b=document.createElement('button');b.className='attachment';b.title=`${a.name} · ${a.width} × ${a.height} · ${Math.round(a.bytes/1024)} KB · 点击移除`;const img=document.createElement('img');img.src=`data:${a.mime};base64,${a.dataBase64}`;img.alt=a.name; b.append(img,document.createTextNode(`${a.name} ×`));b.onclick=()=>{attachments.splice(i,1);renderAttachments()};host.append(b)});
}
async function add(files){
 for(const file of files){
  if(!['image/png','image/jpeg','image/gif','image/webp'].includes(file.type))throw new Error('仅支持 PNG、JPEG、GIF 和 WebP');
  if(file.size>4*1024*1024 || attachments.reduce((s,a)=>s+a.bytes,0)+file.size>5*1024*1024 || attachments.length>=10)throw new Error('单图上限 4 MB，附件总计不超过 5 MB / 10 张');
  const data=await new Promise((resolve,reject)=>{const r=new FileReader();r.onload=()=>resolve(r.result);r.onerror=reject;r.readAsDataURL(file)});
  const image=new Image();image.src=data;await image.decode();if(image.width*image.height>40_000_000)throw new Error('图片尺寸过大');
  attachments.push({mime:file.type,dataBase64:data.split(',')[1],name:file.name,bytes:file.size,width:image.width,height:image.height});
 }
 renderAttachments();
}
window.medsciAttach=()=>{const input=document.createElement('input');input.type='file';input.accept='image/png,image/jpeg,image/gif,image/webp';input.multiple=true;input.onchange=()=>add(input.files).catch(e=>alert(e.message));input.click()};
document.addEventListener('paste',e=>{if(e.clipboardData.files.length){e.preventDefault();add(e.clipboardData.files).catch(e=>alert(e.message))}});
document.addEventListener('dragover',e=>e.preventDefault());document.addEventListener('drop',e=>{e.preventDefault();add(e.dataTransfer.files).catch(e=>alert(e.message))});
document.addEventListener('click',e=>{const a=e.target.closest('a');if(a){e.preventDefault();alert(`链接目标：${a.href}\n请复制到浏览器打开。`)}});
document.addEventListener('keydown',e=>{
 if(window.__TAURI__ && /Mac/.test(navigator.platform))return; // Native menu owns macOS accelerators.
 if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='n'){e.preventDefault();window.medsciInvoke('new_session',{}).catch(e=>alert(String(e)));}
 if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='o'){e.preventDefault();window.medsciInvoke('open_folder',{}).catch(e=>alert(String(e)));}
});
await init();

// Measure the actual line wrapping, including paste, IME and window resizing.
let previousDraft, previousWidth, previousTitle;
function syncLayout(){
 const input=document.querySelector('.composer textarea');
 if(input && (input.value!==previousDraft || input.clientWidth!==previousWidth)){
  previousDraft=input.value;previousWidth=input.clientWidth;
  input.style.height='28px';
  const height=Math.min(180,Math.max(28,input.scrollHeight));
  input.style.height=`${height}px`;input.style.overflowY=input.scrollHeight>180?'auto':'hidden';
 }
 const title=document.querySelector('.workspace')?.textContent || '';
 if(title!==previousTitle){previousTitle=title;document.title=title || 'Codewhale-MedSci';if(window.__TAURI__)window.__TAURI__.window.getCurrentWindow().setTitle(title).catch(console.error);}
 requestAnimationFrame(syncLayout);
}
requestAnimationFrame(syncLayout);
document.addEventListener('click',async e=>{
 const control=e.target.closest('[data-window]');if(!control || !window.__TAURI__)return;
 const w=window.__TAURI__.window.getCurrentWindow();
 try{if(control.dataset.window==='minimize')await w.minimize();if(control.dataset.window==='maximize')await w.toggleMaximize();if(control.dataset.window==='close')await w.close();}catch(error){console.error(error);}
});

// Follow newly appended messages and streaming text without moving keyboard focus.
const conversation=document.querySelector('main');
if(conversation){
 let scrollPending=false;
 const followLatest=()=>{
  if(scrollPending)return;scrollPending=true;
  requestAnimationFrame(()=>{scrollPending=false;conversation.scrollTop=conversation.scrollHeight;});
 };
 new MutationObserver(followLatest).observe(conversation,{childList:true,subtree:true,characterData:true});
 // Images/code wrapping can change layout after their message is inserted.
 conversation.addEventListener('load',followLatest,true);
 followLatest();
}
