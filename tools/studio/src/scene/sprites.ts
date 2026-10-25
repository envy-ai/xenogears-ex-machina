/** Read-only original-part billboard projection. Rust owns resource/VM facts. */
import * as THREE from 'three';
import type { SceneSpriteBinding } from '../backend/scenes';
import type { SceneSpriteResource, SceneSpritePart } from '../backend/scene-assets';
import { SceneVram } from './textures';

// field_actor_create_sprite: shift3; field sprite draw: default actor scale4096
// multiplied by3/4 before the shift. This is initial source scale, not live state.
export const FIELD_SPRITE_PIXEL_SCALE=6;
export class SpritePreviewUnavailable extends Error {}
export interface SpritePartProjection {
  left:number; top:number; width:number; height:number; flipX:boolean; flipY:boolean;
}
export function projectSpritePart(part:SceneSpritePart,flipX:boolean,flipY:boolean):SpritePartProjection {
  const x=part.position[0]*(flipX?-1:1),y=part.position[1]*(flipY?-1:1);
  const width=part.size[0]*(flipX?-1:1),height=part.size[1]*(flipY?-1:1);
  return {left:Math.min(x,x+width),top:Math.min(y,y+height),width:Math.abs(width),height:Math.abs(height),flipX:part.flip_x!==(width<0),flipY:part.flip_y!==(height<0)};
}
export interface SpriteImage { projection:SpritePartProjection; rgba:Uint8Array; width:number; height:number }
export function sourceSpriteImages(resource:SceneSpriteResource,bank:number,base:SceneVram):SpriteImage[] {
  const frame=resource.first_frame;
  if(!frame)throw new SpritePreviewUnavailable(resource.diagnostic??'This source has no qualified initial sprite frame');
  const selected=resource.palette_banks.find(p=>p.bank===bank);
  if(resource.palette_count!==0&&!selected)throw new SpritePreviewUnavailable(`Source sprite slot${resource.slot} has no palette bank${bank}`);
  const x=256+bank*16,y=480+resource.slot;
  if(selected&&x+selected.words.length>1024)throw new SpritePreviewUnavailable('Sprite palette bank exceeds its original VRAM row');
  // ClearImage is an explicit source operation, not pixel bytes or RAM padding.
  const clear=frame.clear?[{rect:frame.clear.rect,words:new Array<number>(frame.clear.rect[2]*frame.clear.rect[3]).fill(0)}]:[];
  const uploads=[...clear,...frame.uploads,...(selected?[{source_span:selected.source_span,pixels_offset:selected.source_span[0],rect:[x,y,selected.words.length,1] as [number,number,number,number],words:selected.words,kind:'sprite_palette',slot:resource.slot,tim_mode:null}]:[])];
  const vram=base.withUploads(uploads);
  return frame.parts.map(part=>{
    if(part.blend)throw new SpritePreviewUnavailable('This source sprite frame uses PS1 semitransparent blending; no opaque substitute is shown');
    const [u,v]=part.uv,[w,h]=part.uv_size;
    if(u+w>256||v+h>256)throw new SpritePreviewUnavailable('This source sprite part crosses a texture-page UV boundary');
    const clut=(part.clut&~63)|((part.clut+bank)&63),page=vram.page(part.tpage,clut);
    const projection=projectSpritePart(part,frame.flip_x,frame.flip_y),rgba=new Uint8Array(w*h*4);
    for(let row=0;row<h;row++)for(let column=0;column<w;column++){
      const source=(v+row)*256+u+column;
      if(!page.known[source])throw new SpritePreviewUnavailable('This source sprite needs image/palette words not uploaded by the selected map; missing pixels are not synthesized');
      const px=projection.flipX?w-1-column:column,py=projection.flipY?h-1-row:row;
      rgba.set(page.rgba.subarray(source*4,source*4+4),(py*w+px)*4);
    }
    return {projection,rgba,width:w,height:h};
  });
}
export interface SourceSpriteModel { group:THREE.Group; textures:THREE.Texture[]; materials:THREE.Material[] }
export function makeSourceSprite(binding:SceneSpriteBinding,resource:SceneSpriteResource,vram:SceneVram):SourceSpriteModel {
  if(binding.bank===null)throw new SpritePreviewUnavailable('The source sprite palette bank is unresolved');
  const images=sourceSpriteImages(resource,binding.bank,vram).filter(image=>image.projection.width>0&&image.projection.height>0),group=new THREE.Group();
  if(!images.some(image=>image.rgba.some((value,index)=>index%4===3&&value!==0)))throw new SpritePreviewUnavailable('The initial source sprite frame contains no visible pixels');
  const textures:THREE.Texture[]=[],materials:THREE.Material[]=[];
  for(const [index,image]of images.entries()){
    const {projection,width,height,rgba}=image;
    const texture=new THREE.DataTexture(rgba,width,height,THREE.RGBAFormat);
    texture.colorSpace=THREE.SRGBColorSpace;texture.minFilter=THREE.NearestFilter;texture.magFilter=THREE.NearestFilter;texture.generateMipmaps=false;
    // Source row0 is top; Three.Sprite's fixed top UV is v=1. Reverse V once
    // here, independently of authored part/frame flips already applied above.
    texture.repeat.y=-1;texture.offset.y=1;texture.updateMatrix();texture.needsUpdate=true;
    const material=new THREE.SpriteMaterial({map:texture,color:0xffffff,alphaTest:0.01,depthTest:true,depthWrite:true});
    const sprite=new THREE.Sprite(material);
    // Anchor each part to the actor's authored origin. Sprite.center is allowed
    // outside0..1; all parts then billboard around the SAME actor pivot.
    sprite.center.set(projection.width?-projection.left/projection.width:0,projection.height?1+projection.top/projection.height:0);
    sprite.scale.set(projection.width*FIELD_SPRITE_PIXEL_SCALE,projection.height*FIELD_SPRITE_PIXEL_SCALE,1);
    // Original addPrim prepends; the first part draws last/on top.
    sprite.renderOrder=images.length-index;group.add(sprite);textures.push(texture);materials.push(material);
  }
  // Conservative camera bounds around the authored pivot; billboard offsets
  // live in the vertex shader and are not reflected in Object3D's geometry box.
  group.userData.spriteRadius=Math.max(1,...images.flatMap(({projection:p})=>[Math.abs(p.left),Math.abs(p.top),Math.abs(p.left+p.width),Math.abs(p.top+p.height)]))*FIELD_SPRITE_PIXEL_SCALE;
  group.userData.sprite={actor:binding.actor,resource:resource.slot,bank:binding.bank,frame:resource.first_frame!.frame,animation:0,facing:0,parts:images.length,pc:binding.pc,node:binding.node};
  return {group,textures,materials};
}
