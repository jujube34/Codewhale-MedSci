import init from './pkg/medsci_ui.js';
window.medsciInvoke = async (command, args) => {
  if (!window.__TAURI__) throw new Error('请在已安装的桌面应用中使用此功能');
  const result = await window.__TAURI__.core.invoke(command,args);
  if(command==='send_message')document.dispatchEvent(new Event('medsci-message-sent'));
  return result;
};
window.medsciNative = () => Boolean(window.__TAURI__);
document.documentElement.classList.toggle('windows', /Win/.test(navigator.platform));
document.documentElement.classList.toggle('macos', /Mac/.test(navigator.platform));
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
let referenceQueue = Promise.resolve(), pendingReferences = 0, nextReferenceId = 0;
window.medsciReferences = () => attachments.map(a => ({...a}));
window.medsciReferencesPending = () => pendingReferences > 0;
window.medsciRemoveReferences = sent => {
 const ids = new Set(JSON.parse(sent).map(a => a.id));
 attachments = attachments.filter(a => !ids.has(a.id));
 referenceNotice();
 renderAttachments();
};
window.medsciCopyText = async text => {
 try {
  if (navigator.clipboard?.writeText) {
   await navigator.clipboard.writeText(String(text));
   return true;
  }
 } catch (_) {}
 const textarea=document.createElement('textarea');
 textarea.value=String(text);textarea.readOnly=true;
 textarea.style.cssText='position:fixed;opacity:0;pointer-events:none';
 document.body.append(textarea);textarea.select();
 const copied=document.execCommand('copy');textarea.remove();
 if(!copied)throw new Error('复制失败');
 return true;
};
function referenceNotice(message='') {
 const notice=document.getElementById('reference-notice');
 if(notice)notice.textContent=message;
}
function renderAttachments(){
 const host=document.getElementById('attachments'); if(!host)return;
 host.replaceChildren();
 for(const a of attachments){
  const label=`${a.isDirectory?'文件夹':'文件'}${String.fromCharCode(65+a.slot)}`;
  const row=document.createElement('div');row.className='file-reference';row.setAttribute('role','listitem');
  const remove=document.createElement('button');remove.type='button';remove.className='remove-reference';
  remove.textContent='×';remove.title=`移除${label}`;remove.setAttribute('aria-label',`移除${label}：${a.path}`);
  remove.onclick=()=>{attachments=attachments.filter(item=>item.id!==a.id);referenceNotice();renderAttachments()};
  const extension=a.path.split(/[\\/]/).pop().split('.').pop().toLowerCase();
  const kind=a.isDirectory?'folder':(['docx','xlsx','pptx','pdf'].includes(extension)?extension:'file');
  const icon=document.createElement('span');icon.className=`reference-icon icon-${kind}`;icon.setAttribute('aria-hidden','true');
  icon.textContent=({docx:'W',xlsx:'X',pptx:'P',pdf:'PDF',folder:'',file:''})[kind];
  const name=document.createElement('span');name.className='reference-name';name.textContent=label;
  const path=document.createElement('span');path.className='reference-path';path.textContent=a.path;path.title=a.path;
  row.append(remove,icon,name,path);host.append(row);
 }
 host.dispatchEvent(new Event('input',{bubbles:true}));
}
function addReferences(files){
 const added=files.filter(file=>!attachments.some(a=>a.path===file.path));
 if(attachments.length+added.length>10)throw new Error('最多引用 10 个文件/文件夹，请先移除部分引用');
 for(const file of added){
  if(attachments.some(a=>a.path===file.path))continue;
  const slot=Array.from({length:10},(_,i)=>i).find(i=>!attachments.some(a=>a.slot===i));
  attachments.push({...file,slot,id:++nextReferenceId});
 }
 referenceNotice();renderAttachments();
}
function queueReferences(task){
 pendingReferences++;renderAttachments();
 referenceQueue=referenceQueue.then(task)
  .catch(error=>referenceNotice(String(error.message||error)))
  .finally(()=>{pendingReferences--;renderAttachments()});
}
// Only the task composer consumes native file paste. Other text fields keep normal paste.
document.addEventListener('paste',event=>{
 const input=event.target;
 if(!(input instanceof HTMLTextAreaElement)||!input.closest('.composer'))return;
 const text=event.clipboardData?.getData('text/plain')||'';
 const hasFiles=Boolean(event.clipboardData?.files.length);
 if(!window.medsciNative()){
  if(hasFiles){event.preventDefault();referenceNotice('请在桌面应用中复制并粘贴文件或文件夹');}
  return;
 }
 event.preventDefault();
 queueReferences(async()=>{
  const files=await window.medsciInvoke('clipboard_file_references',{});
  if(files.length){addReferences(files);return;}
  if(hasFiles){throw new Error('无法获得文件路径，请从文件管理器复制文件或文件夹；不支持直接粘贴截图');}
  if(text&&input.isConnected){
   input.focus();
   // Preserve native undo where available, with a textarea fallback.
   if(!document.execCommand('insertText',false,text)){
    input.setRangeText(text,input.selectionStart,input.selectionEnd,'end');
    input.dispatchEvent(new Event('input',{bubbles:true}));
   }
  }
 });
});
// Webview File objects cannot supply absolute paths; never invent a path from a filename.
document.addEventListener('dragover',event=>{if(event.dataTransfer?.types.includes('Files'))event.preventDefault()});
document.addEventListener('drop',event=>{
 if(!event.dataTransfer?.files.length)return;
 event.preventDefault();
 if(!window.medsciNative())referenceNotice('请在桌面应用中拖入文件或文件夹');
});
document.addEventListener('click',e=>{const a=e.target.closest('a');if(a){e.preventDefault();alert(`链接目标：${a.href}\n请复制到浏览器打开。`)}});
document.addEventListener('keydown',e=>{
 if(window.__TAURI__ && /Mac/.test(navigator.platform))return; // Native menu owns macOS accelerators.
 if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='n'){e.preventDefault();window.medsciInvoke('new_session',{}).catch(e=>alert(String(e)));}
 if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='o'){e.preventDefault();window.medsciInvoke('open_folder',{}).catch(e=>alert(String(e)));}
});
await init();
// Theme is presentation state only; it never changes the agent/session context.
const themeToggle = document.createElement('button');
themeToggle.type = 'button';
themeToggle.className = 'theme-toggle';
themeToggle.textContent = '深色模式';
document.querySelector('.window-controls')?.before(themeToggle);
themeToggle.addEventListener('click', () => {
 const dark = document.documentElement.dataset.theme !== 'dark';
 document.documentElement.dataset.theme = dark ? 'dark' : 'light';
 themeToggle.textContent = dark ? '浅色模式' : '深色模式';
});
if(window.medsciNative()){
 try{
  await window.__TAURI__.webview.getCurrentWebview().onDragDropEvent(({payload})=>{
   const hovering=payload.type==='enter'||payload.type==='over';
   document.querySelector('.composer')?.classList.toggle('file-drag-active',hovering);
   if(payload.type==='enter')referenceNotice('松开即可引用文件或文件夹');
   if(payload.type==='leave'||payload.type==='drop')referenceNotice();
   if(payload.type==='drop'){
    const paths=[...payload.paths];
    queueReferences(async()=>{
     addReferences(await window.medsciInvoke('resolve_file_references',{paths}));
     document.querySelector('.composer textarea')?.focus();
    });
   }
  });
 }catch(error){referenceNotice(`文件拖拽暂不可用：${String(error.message||error)}`);}
}
if (window.__TAURI__ && document.documentElement.classList.contains('windows')) {
 const nativeWindow=window.__TAURI__.window.getCurrentWindow();
 const resizeHandles=document.createElement('div');
 resizeHandles.className='window-resize-handles';
 resizeHandles.setAttribute('aria-hidden','true');
 for(const direction of ['North','South','East','West','NorthEast','NorthWest','SouthEast','SouthWest']){
  const handle=document.createElement('div');
  handle.dataset.resizeDirection=direction;
  handle.addEventListener('pointerdown',event=>{
   if(event.button!==0 || document.documentElement.classList.contains('window-maximized'))return;
   event.preventDefault();event.stopPropagation();
   nativeWindow.startResizeDragging(direction).catch(console.error);
  });
  resizeHandles.append(handle);
 }
 document.body.append(resizeHandles);
 const syncWindowShape=async()=>{
  try { document.documentElement.classList.toggle('window-maximized',await nativeWindow.isMaximized()); }
  catch(error){console.error(error);}
 };
 await syncWindowShape();
 await nativeWindow.onResized(syncWindowShape);
}

// Markdown stays the clipboard source for whole answers; code blocks expose
// their exact code text as a smaller, hover-only copy target.
function decorateCodeBlocks(root=document){
 root.querySelectorAll('pre:not([data-copy-ready])').forEach(pre=>{
  const code=pre.querySelector(':scope > code');if(!code)return;
  pre.dataset.copyReady='true';pre.classList.add('code-block');
  const toolbar=document.createElement('div');toolbar.className='code-toolbar';
  const language=document.createElement('span');language.className='code-language';
  language.textContent=[...code.classList].find(name=>name.startsWith('language-'))?.slice(9)||'';
  const button=document.createElement('button');button.type='button';button.className='copy-code';button.textContent='复制';
  button.onclick=async()=>{try{await window.medsciCopyText(code.textContent||'');button.textContent='已复制';setTimeout(()=>button.textContent='复制',1500)}catch(error){alert(error.message)}};
  toolbar.append(language,button);pre.prepend(toolbar);
 });
}
decorateCodeBlocks();
new MutationObserver(records=>{
 for(const record of records)for(const node of record.addedNodes)if(node instanceof Element){if(node.matches('pre'))decorateCodeBlocks(node.parentElement||document);else decorateCodeBlocks(node)}
}).observe(document.body,{childList:true,subtree:true});

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

// Follow only while the user remains at the tail. Once they scroll up, content
// can stream without stealing their reading position.
const conversation=document.querySelector('main');
if(conversation){
 const latest=document.getElementById('back-to-latest');
 const tailThreshold=48;
 let scrollPending=false,sticky=true;
 const nearTail=()=>conversation.scrollHeight-conversation.scrollTop-conversation.clientHeight<=tailThreshold;
 const syncButton=()=>{if(latest)latest.hidden=sticky};
 const followLatest=()=>{
  if(!sticky||scrollPending)return;scrollPending=true;
  requestAnimationFrame(()=>{scrollPending=false;if(!sticky)return;conversation.scrollTop=conversation.scrollHeight;syncButton()});
 };
 conversation.addEventListener('scroll',()=>{sticky=nearTail();syncButton()},{passive:true});
 latest?.addEventListener('click',()=>{sticky=true;followLatest()});
 document.addEventListener('medsci-message-sent',()=>{sticky=true;followLatest()});
 new MutationObserver(followLatest).observe(conversation,{childList:true,subtree:true,characterData:true});
 // Images/code wrapping can change layout after their message is inserted.
 conversation.addEventListener('load',followLatest,true);
 // Derive navigation from rendered user prompts, including restored history and
 // steer messages. This is presentation only; no extra session/context state.
 const outline=document.createElement('nav');
 outline.className='turn-outline';outline.setAttribute('aria-label','会话轮次');outline.hidden=true;
 const marks=document.createElement('div');marks.className='turn-marks';
 const list=document.createElement('div');list.className='turn-list';
 outline.append(marks,list);
 conversation.after(outline);
 let prompts=[],labels=[],outlinePending=false;
 const syncOutline=()=>{
  outlinePending=false;
  prompts=[...conversation.querySelectorAll('article.user')];
  const nextLabels=prompts.map(prompt=>{
   const content=prompt.cloneNode(true);content.querySelectorAll('.code-toolbar').forEach(toolbar=>toolbar.remove());
   return content.textContent.trim()||'（附件指令）';
  });
  if(nextLabels.length!==labels.length||nextLabels.some((label,index)=>label!==labels[index])){
   labels=nextLabels;
   for(const host of [marks,list])host.replaceChildren(...labels.map((label,index)=>{
    const button=document.createElement('button');button.type='button';
    button.className='turn-marker';button.title=label;
    button.dataset.turn=index;
    if(host===marks)button.tabIndex=-1;
    button.setAttribute('aria-label',`第 ${index+1} 轮：${label}`);
    const line=document.createElement('span');line.className='turn-line';line.setAttribute('aria-hidden','true');
    const text=document.createElement('span');text.className='turn-label';text.textContent=label;
    button.append(line,text);
    button.onclick=()=>{
     const prompt=prompts[index];if(!prompt)return;
     // Stop an already queued followLatest before navigating away from the tail.
     sticky=false;syncButton();
     conversation.scrollTo({top:conversation.scrollTop+prompt.getBoundingClientRect().top-conversation.getBoundingClientRect().top-24,behavior:'instant'});
     scheduleOutline();
    };
    return button;
   }));
  }
  outline.hidden=prompts.length<2;
  conversation.classList.toggle('has-turn-outline',!outline.hidden);
  outline.style.top=`${conversation.offsetTop+conversation.clientHeight/2}px`;
  outline.style.maxHeight=`${Math.max(0,conversation.clientHeight-32)}px`;
  const readingTop=conversation.getBoundingClientRect().top+48;
  let current=0;
  prompts.forEach((prompt,index)=>{if(prompt.getBoundingClientRect().top<=readingTop)current=index});
  if(conversation.scrollTop>0&&conversation.scrollHeight-conversation.scrollTop-conversation.clientHeight<=2)current=prompts.length-1;
  outline.querySelectorAll('.turn-marker').forEach(button=>{
   if(Number(button.dataset.turn)===current)button.setAttribute('aria-current','location');else button.removeAttribute('aria-current');
   if(button.parentElement===marks)button.tabIndex=Number(button.dataset.turn)===current?0:-1;
  });
  for(const host of [marks,list]){
   if(host===marks||!outline.matches(':hover, :focus-within')){
    const active=host.children[current];
    if(active)host.scrollTop=active.offsetTop-host.clientHeight/2+active.offsetHeight/2;
   }
  }
 };
 const scheduleOutline=()=>{if(!outlinePending){outlinePending=true;requestAnimationFrame(syncOutline)}};
 conversation.addEventListener('scroll',scheduleOutline,{passive:true});
 conversation.addEventListener('load',scheduleOutline,true);
 conversation.addEventListener('toggle',scheduleOutline,true);
 new MutationObserver(scheduleOutline).observe(conversation,{childList:true,subtree:true,characterData:true});
 new ResizeObserver(scheduleOutline).observe(conversation);
 outline.addEventListener('keydown',event=>{
  const index=Number(document.activeElement.dataset.turn);
  const target=event.key==='ArrowDown'?index+1:event.key==='ArrowUp'?index-1:event.key==='Home'?0:event.key==='End'?labels.length-1:null;
  if(target!==null){event.preventDefault();list.children[Math.max(0,Math.min(labels.length-1,target))]?.focus()}
  if(event.key==='Escape'){document.activeElement.blur();outline.classList.add('dismissed')}
 });
 outline.addEventListener('pointerleave',()=>outline.classList.remove('dismissed'));
 outline.addEventListener('focusin',()=>outline.classList.remove('dismissed'));
 scheduleOutline();
 followLatest();
}
