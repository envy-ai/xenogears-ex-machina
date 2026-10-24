/** Source upload projection. Pixel addressing matches xem-render/ps1/draw.wgsl.
 * Only known uploaded words may supply a texel; missing VRAM is explicit. */
import type { SceneAssets } from '../backend/scene-assets';
export interface TexturePage { rgba:Uint8Array; known:Uint8Array }
export class SceneVram {
  private words=new Uint16Array(1024*512);
  private known=new Uint8Array(1024*512);
  private pages=new Map<string,TexturePage>();
  constructor(assets:Pick<SceneAssets,'palettes'|'images'>) {
    // field_load_from_bundle uploads palette TIMs before the image slot list.
    for(const upload of [...assets.palettes.uploads,...assets.images.uploads]) {
      const [x,y,w,h]=upload.rect;
      if(upload.words.length!==w*h)throw new Error('Decoded VRAM upload rectangle and words disagree');
      for(let row=0;row<h;row++)for(let column=0;column<w;column++){
        const at=((y+row)&511)*1024+((x+column)&1023);
        this.words[at]=upload.words[row*w+column];this.known[at]=1;
      }
    }
  }
  private read(x:number,y:number):number|null {
    const at=(y&511)*1024+(x&1023);return this.known[at]?this.words[at]:null;
  }
  page(tpage:number,clut:number):TexturePage {
    const key=`${tpage}:${clut}`;const previous=this.pages.get(key);if(previous)return previous;
    if(this.pages.size>=256)throw new Error('Source texture preview exceeds256 explicit page/CLUT combinations');
    const rgba=new Uint8Array(256*256*4),known=new Uint8Array(256*256);
    const depth=(tpage>>>7)&3,px=(tpage&15)*64,py=((tpage>>>4)&1)*256,cx=(clut&63)*16,cy=(clut>>>6)&511;
    for(let v=0;v<256;v++)for(let u=0;u<256;u++){
      const packed=this.read(px+(depth===0?u>>>2:depth===1?u>>>1:u),py+v);
      if(packed===null)continue;
      const color=depth===0?this.read(cx+((packed>>>((u&3)*4))&15),cy):depth===1?this.read(cx+((packed>>>((u&1)*8))&255),cy):packed;
      if(color===null)continue;
      const at=v*256+u;known[at]=1;
      rgba[at*4]=Math.round((color&31)*255/31);rgba[at*4+1]=Math.round(((color>>>5)&31)*255/31);rgba[at*4+2]=Math.round(((color>>>10)&31)*255/31);rgba[at*4+3]=color===0?0:255;
    }
    const page={rgba,known};this.pages.set(key,page);return page;
  }
}
export function facePixelsKnown(page:TexturePage,uv:readonly [number,number][]):boolean {
  const us=uv.map(p=>p[0]),vs=uv.map(p=>p[1]);
  for(let v=Math.min(...vs);v<=Math.max(...vs);v++)for(let u=Math.min(...us);u<=Math.max(...us);u++)if(!page.known[v*256+u])return false;
  return true;
}
