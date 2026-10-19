"use strict";
// Scene transfer and draft color tools share the existing native library.
window.LibraryTools = (() => {
  const el = id => document.getElementById(id);
  const hex = rgb => '#' + rgb.map(v => Math.round(v).toString(16).padStart(2, '0')).join('');
  const rgb = color => [1,3,5].map(i => parseInt(color.slice(i,i+2),16));
  let draft = '#ffffff', original, hsl = [0,0,100], colors = [], selected = 0, paletteId = '', preview, lastColor, wheelBase;
  function toHsl(color) {
    const [r,g,b] = rgb(color).map(v=>v/255), max=Math.max(r,g,b), min=Math.min(r,g,b), delta=max-min, light=(max+min)/2;
    const hue = !delta ? 0 : max===r ? ((g-b)/delta+6)%6 : max===g ? (b-r)/delta+2 : (r-g)/delta+4;
    return [hue*60,delta ? delta/(1-Math.abs(2*light-1))*100 : 0,light*100];
  }
  function fromHsl(h,s,l) {
    s/=100; l/=100; const c=(1-Math.abs(2*l-1))*s, x=c*(1-Math.abs((h/60)%2-1)), m=l-c/2;
    return hex((h<60?[c,x,0]:h<120?[x,c,0]:h<180?[0,c,x]:h<240?[0,x,c]:h<300?[x,0,c]:[c,0,x]).map(v=>(v+m)*255));
  }
  function setDraft(color, preserveHue=false) { draft=color.toLowerCase(); if(!preserveHue)hsl=toHsl(draft); el('color-temperature-value').textContent='—'; renderDraft(); }
  function renderDraft() {
    el('color-hex').value=draft; ['red','green','blue'].forEach((c,i)=>el('color-'+c).value=rgb(draft)[i]);
    ['hue','sat','light'].forEach((c,i)=>{el('color-'+c).value=Math.round(hsl[i]);el('color-'+c+'-value').textContent=Math.round(hsl[i])+(i?'%':'°');});
    const canvas=el('color-preview'), ctx=canvas.getContext('2d');ctx.fillStyle=draft;ctx.fillRect(0,0,canvas.width,canvas.height);
    const wheel=el('color-wheel'), wc=wheel.getContext('2d'), radius=wheel.width/2;
    // Keep hue choices visible even when the current color is white or black.
    const light=50;
    if(wheelBase?.light!==light){const data=wc.createImageData(wheel.width,wheel.height);
    for(let y=0;y<wheel.height;y++)for(let x=0;x<wheel.width;x++){
      const dx=x-radius,dy=y-radius, distance=Math.hypot(dx,dy)/radius;if(distance>1)continue;
      const color=rgb(fromHsl((Math.atan2(dy,dx)*180/Math.PI+360)%360,distance*100,light));
      const offset=(y*wheel.width+x)*4;data.data.set([...color,255],offset);
    }wheelBase={light,data};}
    wc.putImageData(wheelBase.data,0,0);const angle=hsl[0]*Math.PI/180;
    wc.beginPath();wc.arc(radius+Math.cos(angle)*hsl[1]/100*(radius-5),radius+Math.sin(angle)*hsl[1]/100*(radius-5),5,0,Math.PI*2);wc.strokeStyle='#000';wc.lineWidth=4;wc.stroke();wc.strokeStyle='#fff';wc.lineWidth=2;wc.stroke();
  }
  function chip(color, label, action, active=false) {
    const button=document.createElement('button');button.className='color-chip';button.setAttribute('aria-label',label);button.setAttribute('aria-pressed',String(active));button.onclick=action;
    const canvas=document.createElement('canvas');canvas.width=28;canvas.height=28;canvas.setAttribute('aria-hidden','true');const ctx=canvas.getContext('2d');ctx.fillStyle=color;ctx.fillRect(0,0,28,28);button.append(canvas);return button;
  }
  function renderPalette() {
    el('palette-colors').replaceChildren(...colors.map((color,i)=>chip(color,`Color ${i+1}: ${color}`,()=>{selected=i;setDraft(color);renderPalette();},i===selected)));
    el('palette-add').disabled=colors.length>=8;el('palette-remove').disabled=colors.length<=1;el('palette-left').disabled=selected<=0;el('palette-right').disabled=selected>=colors.length-1;
    const canUse=settings.mode==='music'||settings.mode==='rainbow'&&!['rainbow','ember'].includes(settings.effect);
    el('palette-apply').disabled=!canUse;el('palette-delete').disabled=!paletteId;
    el('palette-status').textContent=canUse?'1–8 colors. Order controls the gradient.':'Save a palette here; choose Music, Aurora, Breathe or Comet to use it.';
  }
  function sync() {
    if (!library) return;
    const palettes=library.palettes||[];
    el('recent-colors').replaceChildren(...(library.recent_colors||[]).map(c=>chip(c,'Use recent color '+c,()=>{settings.color=c;changed(['color']);})));
    const select=el('saved-palette'); select.replaceChildren(new Option('New palette',''),...palettes.map(p=>new Option(p.name,p.id)));select.value=paletteId;
    const option=document.querySelector('#palette option[value="custom"]');option.disabled=!settings.customPalette?.length;
  }
  async function remember(value) {
    if(value.settings.color===lastColor)return;
    library=await invoke('remember_color',{color:value.settings.color});lastColor=value.settings.color;sync();
  }
  function open(palettes) {
    original=settings.color;setDraft(original);paletteId='';colors=settings.customPalette?.length?[...settings.customPalette]:[original];selected=0;
    el('palette-name').value='';el('saved-palettes').open=palettes;sync();renderPalette();el('color-editor').showModal();
  }
  async function dialogTask(button,status,task) {button.disabled=true;try{await task();}catch(e){el(status).textContent=String(e);}finally{button.disabled=false;}}
  function init() {
    lastColor=settings.color;sync();
    el('open-color-editor').onclick=()=>open(false);el('open-palette-editor').onclick=()=>open(true);
    el('close-color-editor').onclick=()=>el('color-editor').close();
    el('reset-color').onclick=()=>setDraft(original);
    el('apply-color').onclick=()=>{if(!['hex','red','green','blue'].every(c=>el('color-'+c).reportValidity()))return;settings.color=draft;changed(['color']);el('color-editor').close();};
    el('color-hex').oninput=e=>{if(/^#[0-9a-f]{6}$/i.test(e.target.value))setDraft(e.target.value);};
    ['red','green','blue'].forEach(c=>el('color-'+c).oninput=()=>{const fields=['red','green','blue'].map(k=>el('color-'+k));if(fields.every(f=>f.value!==''&&f.checkValidity()))setDraft(hex(fields.map(f=>+f.value)));});
    ['hue','sat','light'].forEach((c,i)=>el('color-'+c).oninput=e=>{hsl[i]=+e.target.value;setDraft(fromHsl(...hsl),true);});
    const pick=e=>{const rect=el('color-wheel').getBoundingClientRect(),dx=(e.clientX-rect.left)/rect.width-.5,dy=(e.clientY-rect.top)/rect.height-.5;hsl[0]=(Math.atan2(dy,dx)*180/Math.PI+360)%360;hsl[1]=Math.min(100,Math.hypot(dx,dy)*200);if(hsl[2]===0||hsl[2]===100)hsl[2]=50;setDraft(fromHsl(...hsl),true);};
    el('color-wheel').onpointerdown=e=>{el('color-wheel').setPointerCapture(e.pointerId);pick(e);};el('color-wheel').onpointermove=e=>{if(el('color-wheel').hasPointerCapture(e.pointerId))pick(e);};
    el('color-temperature').oninput=e=>{
      // Deliberate RGB approximation between warm/cool reference swatches.
      const stops=[[1800,[255,126,0]],[2700,[255,167,87]],[4000,[255,206,166]],[6500,[255,249,253]],[10000,[202,218,255]]],k=+e.target.value;
      const i=Math.max(1,stops.findIndex(([t])=>t>=k)),a=stops[i-1],b=stops[i],t=(k-a[0])/(b[0]-a[0]);
      setDraft(hex(a[1].map((v,j)=>v+(b[1][j]-v)*t)));el('color-temperature-value').textContent=k+' K';
    };
    el('saved-palette').onchange=e=>{paletteId=e.target.value;const p=(library.palettes||[]).find(p=>p.id===paletteId);colors=p?[...p.colors]:[draft];selected=0;el('palette-name').value=p?.name||'';renderPalette();};
    el('palette-add').onclick=()=>{if(colors.length<8){colors.push(draft);selected=colors.length-1;renderPalette();}};
    el('palette-replace').onclick=()=>{colors[selected]=draft;renderPalette();};
    el('palette-remove').onclick=()=>{if(colors.length>1){colors.splice(selected,1);selected=Math.min(selected,colors.length-1);renderPalette();}};
    for(const [id,delta] of [['palette-left',-1],['palette-right',1]])el(id).onclick=()=>{const next=selected+delta;if(next>=0&&next<colors.length){[colors[selected],colors[next]]=[colors[next],colors[selected]];selected=next;renderPalette();}};
    el('palette-save').onclick=()=>dialogTask(el('palette-save'),'palette-status',async()=>{
      const name=el('palette-name').value.trim();if(!name)throw Error('Enter a palette name.');
      const id=paletteId||crypto.randomUUID();await editLibrary(next=>{next.palettes||=[];const p={id,name,colors:[...colors]};const i=next.palettes.findIndex(p=>p.id===id);if(i<0)next.palettes.push(p);else next.palettes[i]=p;});paletteId=id;sync();renderPalette();el('palette-status').textContent='Palette saved.';
    });
    el('palette-delete').onclick=()=>dialogTask(el('palette-delete'),'palette-status',async()=>{await editLibrary(next=>{next.palettes=next.palettes.filter(p=>p.id!==paletteId);});paletteId='';el('palette-name').value='';sync();renderPalette();});
    el('palette-apply').onclick=()=>{settings.palette='custom';settings.customPalette=[...colors];changed(['palette','customPalette']);el('color-editor').close();};
    el('share-scenes').onclick=()=>{
      el('export-items').replaceChildren();el('export-status').textContent='';
      for(const [kind,items] of [['profile',library.profiles],['palette',library.palettes||[]]])for(const item of items){const label=document.createElement('label');label.className='checkbox';const input=document.createElement('input');input.type='checkbox';input.checked=true;input.dataset.kind=kind;input.value=item.id;label.append(input,document.createTextNode(item.name+' · '+kind));el('export-items').append(label);}
      if(!el('export-items').children.length)el('export-status').textContent='Save a profile or palette to export it.';el('export-dialog').showModal();
    };
    el('export-cancel').onclick=()=>el('export-dialog').close();
    el('export-save').onclick=()=>dialogTask(el('export-save'),'export-status',async()=>{const checks=[...el('export-items').querySelectorAll('input:checked')];if(!checks.length)throw Error('Select at least one profile or palette.');const path=await invoke('export_profiles',{ids:checks.filter(c=>c.dataset.kind==='profile').map(c=>c.value),paletteIds:checks.filter(c=>c.dataset.kind==='palette').map(c=>c.value)});el('export-status').textContent='Saved to '+path;});
    el('import-scenes').onclick=()=>el('scene-file').click();
    el('scene-file').onchange=async e=>{
      const file=e.target.files[0];if(!file)return;
      try{
        if(file.size>512*1024)throw Error('Scene files must be smaller than 512 KiB.');
        preview=await invoke('preview_profile_import',{text:await file.text()});el('import-items').replaceChildren();
        for(const [items,matches,kind] of [[preview.bundle.profiles,preview.conflicts,'Scene'],[preview.bundle.palettes,preview.palette_conflicts,'Palette']])items.forEach((p,i)=>{const row=document.createElement('p');row.textContent=p.name+' · '+kind+(matches[i]?' · matches '+matches[i]:' · new');el('import-items').append(row);});
        el('import-policy').value='copy';el('import-status').textContent='';el('import-dialog').showModal();
      }catch(error){el('sharing-status').textContent=String(error);}finally{e.target.value='';}
    };
    el('import-cancel').onclick=()=>{preview=null;el('import-dialog').close();};
    el('import-confirm').onclick=()=>dialogTask(el('import-confirm'),'import-status',async()=>{if(!preview)return;library=await invoke('import_profiles',{bundle:preview.bundle,expected:preview.base,policy:el('import-policy').value});preview=null;renderProfiles();sync();el('import-dialog').close();el('sharing-status').textContent='Imported. Your previous library is backed up on this computer.';});
  }
  return {init,sync,remember};
})();
