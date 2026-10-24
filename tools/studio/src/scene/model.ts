/** Editor-only projection of decoded source assets. It never edits game bytes. */
import * as THREE from 'three';
import type { SceneAssets } from '../backend/scene-assets';
import type { SceneSnapshot, SceneVector } from '../backend/scenes';
import type { SceneCanvasModel } from '../editor/SceneCanvas';
import { SceneVram, facePixelsKnown } from './textures';
import type { ScenePose } from '../game/scene-pose';
export type SceneLayer='geometry'|'actors'|'collision'|'zones'|'entrances';
export interface SceneRow {id:string;label:string;layer:SceneLayer;kind:'descriptor'|'actor'|'vertex'|'triangle'|'zone'|'entrance';position:SceneVector;rotation:SceneVector;index:number;parent?:string}
export const toView=(point:readonly number[]):SceneVector=>[point[0],-point[1],point[2]];
export const fromView=(point:readonly number[]):SceneVector=>[point[0],-point[1],point[2]];
export function sceneRows(state:SceneSnapshot):SceneRow[]{
  const rows:SceneRow[]=state.descriptors.map((item,index)=>({id:item.id,label:`${index<state.actors.length?'Actor':'Instance'} ${item.id}`,layer:index<state.actors.length?'actors':'geometry',kind:index<state.actors.length?'actor':'descriptor',position:item.value.position,rotation:item.value.rotation,index}));
  for(const layer of state.collision_entities.layers){
    layer.triangles.forEach((face,index)=>{const vertices=face.value.vertices.map(id=>layer.vertices.find(v=>v.id===id)!.value);rows.push({id:face.id,label:`Face ${index}`,layer:'collision',kind:'triangle',position:[0,1,2].map(axis=>vertices.reduce((sum,v)=>sum+v[axis],0)/3) as SceneVector,rotation:[0,0,0],index,parent:layer.id});});
    layer.vertices.forEach((vertex,index)=>rows.push({id:vertex.id,label:`Vertex ${index}`,layer:'collision',kind:'vertex',position:vertex.value.slice(0,3) as SceneVector,rotation:[0,0,0],index,parent:layer.id}));
  }
  state.zones.forEach((zone,index)=>rows.push({id:zone.id,label:`Zone ${zone.id}`,layer:'zones',kind:'zone',position:[0,1,2].map(axis=>zone.value.corners.reduce((sum,v)=>sum+v[axis],0)/4) as SceneVector,rotation:[0,0,0],index}));
  state.entrances.forEach((entry,index)=>rows.push({id:`entrance:${entry.selector}`,label:`Entry ${entry.selector}`,layer:'entrances',kind:'entrance',position:[entry.x,0,entry.z],rotation:[0,0,0],index}));
  return rows;
}
export interface FieldSceneModel extends SceneCanvasModel {diagnostics:string[]}
export function makeSceneModel(assets:SceneAssets,state:SceneSnapshot,visible:Record<SceneLayer,boolean>,selected:string|null,actorPositions:Map<string,SceneVector>=new Map(),runtimePose:ScenePose|null=null):FieldSceneModel{
  const group=new THREE.Group(),objects=new Map<string,THREE.Object3D>();group.userData.source=assets.source_sha256;
  const geometries=new Set<THREE.BufferGeometry>(),materials=new Set<THREE.Material>(),textures=new Set<THREE.Texture>(),diagnostics=new Set<string>();
  const rememberGeometry=<T extends THREE.BufferGeometry>(geometry:T):T=>{geometries.add(geometry);return geometry;};
  const rememberMaterial=<T extends THREE.Material>(material:T):T=>{materials.add(material);return material;};
  const flat=rememberMaterial(new THREE.MeshBasicMaterial({color:0x84979c,side:THREE.DoubleSide}));
  const vram=new SceneVram(assets),textureMaterials=new Map<string,THREE.MeshBasicMaterial>();
  const meshGroups=assets.geometry.groups.map(source=>{
    const result=new THREE.Group();
    for(const mesh of source.meshes){
      const buckets=new Map<THREE.Material,{positions:number[];uv:number[]}>();
      for(const face of mesh.faces){
        let material:THREE.Material=flat;
        if(face.diagnostic)diagnostics.add(face.diagnostic);
        if(face.uv&&face.declared_tpage!==null&&face.declared_clut!==null){
          const key=`${face.declared_tpage}:${face.declared_clut}`,page=vram.page(face.declared_tpage,face.declared_clut);
          if(facePixelsKnown(page,face.uv)){
            let found=textureMaterials.get(key);
            if(!found){const texture=new THREE.DataTexture(page.rgba,256,256,THREE.RGBAFormat);texture.colorSpace=THREE.SRGBColorSpace;texture.minFilter=THREE.NearestFilter;texture.magFilter=THREE.NearestFilter;texture.generateMipmaps=false;texture.needsUpdate=true;textures.add(texture);found=rememberMaterial(new THREE.MeshBasicMaterial({map:texture,side:THREE.DoubleSide,alphaTest:0.01}));textureMaterials.set(key,found);}
            material=found;
          }else diagnostics.add('A face requires VRAM pixels not supplied by this source; its preview uses an untextured editor material.');
        }
        let bucket=buckets.get(material);if(!bucket){bucket={positions:[],uv:[]};buckets.set(material,bucket);}
        const order=face.vertices.length===4?[0,1,2,1,3,2]:[0,1,2];
        for(const corner of order){bucket.positions.push(...toView(mesh.vertices[face.vertices[corner]]));const uv=face.uv?.[corner]??[0,0];bucket.uv.push((uv[0]+0.5)/256,(uv[1]+0.5)/256);}
      }
      for(const [material,bucket]of buckets){const geometry=rememberGeometry(new THREE.BufferGeometry());geometry.setAttribute('position',new THREE.Float32BufferAttribute(bucket.positions,3));geometry.setAttribute('uv',new THREE.Float32BufferAttribute(bucket.uv,2));result.add(new THREE.Mesh(geometry,material));}
    }
    return result;
  });
  const marker=rememberGeometry(new THREE.CapsuleGeometry(6,16,2,6)),vertexMarker=rememberGeometry(new THREE.SphereGeometry(2,6,4));
  const actorMaterial=rememberMaterial(new THREE.MeshBasicMaterial({color:0x45dddd,wireframe:true}));
  const pointMaterial=rememberMaterial(new THREE.MeshBasicMaterial({color:0x9fefef,depthTest:false}));
  const rows=sceneRows(state);
  for(const row of rows.filter(row=>row.kind==='actor'||row.kind==='descriptor')){
    const descriptor=state.descriptors[row.index].value;
    const object=descriptor.flags&0x40?new THREE.Group():meshGroups[descriptor.model]?.clone(true);
    if(!object)throw new Error(`Decoded descriptor ${row.id} has no source model`);
    if(descriptor.flags&0x40){const body=new THREE.Mesh(marker,actorMaterial);body.position.y=14;object.add(body);}
    object.position.fromArray(toView(actorPositions.get(row.id)??row.position));object.rotation.set(-row.rotation[0]*Math.PI/2048,row.rotation[1]*Math.PI/2048,-row.rotation[2]*Math.PI/2048,'ZYX');
    object.userData.entity=row.id;object.visible=visible[row.layer];objects.set(row.id,object);group.add(object);
  }
  const collisionMaterial=rememberMaterial(new THREE.MeshBasicMaterial({color:0x3ad9df,transparent:true,opacity:0.18,side:THREE.DoubleSide,depthWrite:false}));
  const collisionLine=rememberMaterial(new THREE.LineBasicMaterial({color:0x3ad9df,transparent:true,opacity:0.7}));
  for(const layer of state.collision_entities.layers){
    const positions:number[]=[],ids:string[]=[];
    for(const face of layer.triangles){for(const id of face.value.vertices)positions.push(...toView(layer.vertices.find(v=>v.id===id)!.value));ids.push(face.id);}
    const geometry=rememberGeometry(new THREE.BufferGeometry());geometry.setAttribute('position',new THREE.Float32BufferAttribute(positions,3));
    const mesh=new THREE.Mesh(geometry,collisionMaterial);mesh.visible=visible.collision;mesh.userData.faceEntities=ids;group.add(mesh);
    const wire=new THREE.LineSegments(rememberGeometry(new THREE.EdgesGeometry(geometry,0)),collisionLine);wire.visible=visible.collision;group.add(wire);
    for(const row of rows.filter(row=>row.parent===layer.id)){
      const anchor=new THREE.Group();anchor.position.fromArray(toView(row.position));anchor.userData.entity=row.id;anchor.visible=visible.collision;
      if(row.kind==='vertex'){const dot=new THREE.Mesh(vertexMarker,pointMaterial);anchor.add(dot);}
      else if(row.id===selected){const face=layer.triangles.find(t=>t.id===row.id)!;const points=face.value.vertices.map(id=>new THREE.Vector3(...toView(layer.vertices.find(v=>v.id===id)!.value)).sub(anchor.position));anchor.add(new THREE.LineLoop(rememberGeometry(new THREE.BufferGeometry().setFromPoints(points)),collisionLine));}
      objects.set(row.id,anchor);group.add(anchor);
    }
  }
  const zoneMaterial=rememberMaterial(new THREE.MeshBasicMaterial({color:0x65e787,transparent:true,opacity:0.16,side:THREE.DoubleSide,depthWrite:false}));
  const zoneLine=rememberMaterial(new THREE.LineBasicMaterial({color:0x65e787}));
  for(const [index,item]of state.zones.entries()){
    const row=rows.find(row=>row.id===item.id)!,anchor=new THREE.Group();anchor.position.fromArray(toView(row.position));anchor.userData.entity=item.id;anchor.visible=visible.zones;
    const points=item.value.corners.map(p=>new THREE.Vector3(...toView(p)).sub(anchor.position));
    const geometry=rememberGeometry(new THREE.BufferGeometry().setFromPoints([points[0],points[1],points[2],points[0],points[2],points[3]]));anchor.add(new THREE.Mesh(geometry,zoneMaterial));anchor.add(new THREE.LineLoop(rememberGeometry(new THREE.BufferGeometry().setFromPoints(points)),zoneLine));anchor.userData.zone=index;objects.set(item.id,anchor);group.add(anchor);
  }
  const entranceMaterial=rememberMaterial(new THREE.MeshBasicMaterial({color:0xf598bd,wireframe:true}));
  for(const row of rows.filter(row=>row.kind==='entrance')){const anchor=new THREE.Mesh(rememberGeometry(new THREE.ConeGeometry(8,20,4)),entranceMaterial);anchor.position.fromArray(toView(row.position));anchor.userData.entity=row.id;anchor.visible=visible.entrances;objects.set(row.id,anchor);group.add(anchor);}
  if(runtimePose){
    const material=rememberMaterial(new THREE.MeshBasicMaterial({color:0xffb15c,wireframe:true,depthTest:false}));
    const marker=new THREE.Mesh(rememberGeometry(new THREE.CapsuleGeometry(6,16,2,6)),material);
    marker.position.fromArray(toView(runtimePose.position));marker.position.y+=14;marker.renderOrder=2;group.add(marker);
    diagnostics.add(`Read-only actor pose observed at VBlank ${runtimePose.vblank}; an identity marker, not evaluated sprite geometry.`);
  }
  if(selected&&objects.has(selected)){const helper=new THREE.BoxHelper(objects.get(selected)!,0x05edeb);helper.material.depthTest=false;geometries.add(helper.geometry);materials.add(helper.material);group.add(helper);}
  diagnostics.add('Base source preview: original Y-down units; runtime lighting, texture windows/semitransparent blending, morphs and sprite VM poses are not evaluated. Wire actor markers are source identities.');
  if(state.entrances.length)diagnostics.add('Entry markers show source X/Z on the editor plane; collision-derived height is evaluated by the game.');
  return{group,objects,diagnostics:[...diagnostics],dispose(){geometries.forEach(g=>g.dispose());materials.forEach(m=>m.dispose());textures.forEach(t=>t.dispose());}};
}
