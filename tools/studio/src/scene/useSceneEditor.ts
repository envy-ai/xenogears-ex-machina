/** Inline field drafts over the one Rust-owned authored scene. */
import { useEffect, useMemo, useRef, useState } from 'react';
import type { useFieldDraft } from '../events/useFieldDraft';
import type { DraftSnapshot } from '../backend/drafts';
import type { SceneAction, SceneCorners, ScenePlacement, SceneVector } from '../backend/scenes';
import type { SceneTool, SceneTransform } from '../editor/SceneCanvas';
import { sceneRows, fromView } from './model';
const handlers=new Set(['field_event_place','field_event_place_on_layer','field_event_place_at_height','field_event_set_position']);
type Draft=ReturnType<typeof useFieldDraft>;
const finite=(values:readonly number[],min:number,max:number)=>values.every(v=>Number.isSafeInteger(v)&&v>=min&&v<=max);
export function useSceneEditor(draft:Draft,onSnapshot:(snapshot:DraftSnapshot|null)=>void,onScript:(actor:number,event:number,pc:number,reveal?:boolean)=>boolean,scriptSelection:{actor:number;event:number;pc:number|null}){
  const state=draft.sceneSnapshot;
  const rows=useMemo(()=>state?sceneRows(state):[],[state]);
  const [selected,setSelected]=useState<string|null>(null),[binding,setBinding]=useState('header');
  const selectionRef=useRef({selected,binding});selectionRef.current={selected,binding};
  const [tool,setTool]=useState<SceneTool>('select'),[frame,setFrame]=useState(0),[filter,setFilter]=useState('');
  const [visible,setVisible]=useState({geometry:true,actors:true,collision:true,zones:true,entrances:true});
  const [position,setPosition]=useState<SceneVector>([0,0,0]),[rotation,setRotation]=useState<SceneVector>([0,0,0]);
  const [corners,setCorners]=useState<SceneCorners>([[0,0,0],[0,0,0],[0,0,0],[0,0,0]]);
  const [newFace,setNewFace]=useState(false),[faceVertices,setFaceVertices]=useState<[string,string,string]>(['','','']);
  const [failure,setFailure]=useState<Error|null>(null);
  const row=rows.find(row=>row.id===selected)??null;
  useEffect(()=>{setSelected(state?.actors[scriptSelection.actor]?.id??state?.descriptors[0]?.id??null);setBinding('header');setFailure(null);},[state?.source_sha256]);
  const bindings=useMemo(()=>{
    const values:ScenePlacement[]=[],errors:Error[]=[];
    if(!state||row?.kind!=='actor')return{values,errors};
    const nodes=new Map(Object.entries(state.events.bindings).map(([id,pc])=>[pc,id]));
    for(const ins of state.events.graph.instructions){
      if(!handlers.has(ins.spec.handler))continue;const id=nodes.get(ins.pc);if(!id)continue;
      try{const value=draft.placement(id);if(value.owners.some(owner=>owner.actor===row.id))values.push(value);}catch(cause){errors.push(cause instanceof Error?cause:new Error(String(cause)));}
    }
    return{values,errors};
  },[state,row?.id,draft.placement]);
  const placement=bindings.values.find(value=>value.node===binding)??null;
  useEffect(()=>{
    if(!row||!state)return;
    setPosition(placement?placement.position.map(axis=>axis.kind==='immediate'?axis.value:NaN) as SceneVector:row.kind==='zone'||row.kind==='triangle'?[0,0,0]:row.position);
    setRotation(row.rotation);if(row.kind==='zone')setCorners(state.zones[row.index].value.corners);if(row.kind==='triangle')setFaceVertices(state.collision_entities.layers.find(layer=>layer.id===row.parent)!.triangles[row.index].value.vertices);setNewFace(false);setFailure(null);
  },[selected,binding,draft.editorKey,state?.revision]);
  const axes:[boolean,boolean,boolean]=placement?placement.position.map(axis=>placement.can_transform&&axis.kind==='immediate') as [boolean,boolean,boolean]:row?.kind==='entrance'?[true,false,true]:[true,true,true];
  const canRotate=Boolean(row&&(row.kind==='descriptor'||row.kind==='actor')&&binding==='header'&&state&&!(state.descriptors[row.index].value.flags&0x40));
  const positionMaximum=placement||!(row?.kind==='descriptor'||row?.kind==='actor')?32767:65535;
  const positionMinimum=positionMaximum===65535?0:-32768;
  const invalidPosition=position.map((value,axis)=>axes[axis]&&!finite([value],positionMinimum,positionMaximum));
  const invalidRotation=rotation.map(value=>!finite([value],0,65535));
  const invalidCorners=corners.map(corner=>corner.map(value=>!finite([value],-32768,32767)));
  let validation:string|null=null;
  if(row){
    if(placement){if(!placement.can_transform)validation=placement.diagnostics.join('; ')||'This source placement cannot be transformed';else if(invalidPosition.some(Boolean))validation='Immediate script coordinates must be whole signed16 values';}
    else if(row.kind==='descriptor'||row.kind==='actor'){if(invalidPosition.some(Boolean)||invalidRotation.some(Boolean))validation='Descriptor position/rotation fields must be whole unsigned16 source words';}
    else if(row.kind==='zone'){if(invalidCorners.some(corner=>corner.some(Boolean)))validation='Zone corners must be whole signed16 coordinates';}
    else if(invalidPosition.some(Boolean))validation='Coordinates/translations must be whole signed16 values';
  }
  if(newFace&&(!faceVertices.every(Boolean)||new Set(faceVertices).size!==3))validation='Choose three distinct source vertex identities';
  const perform=(action:SceneAction)=>{try{onSnapshot(draft.sceneEdit(action));setFailure(null);setNewFace(false);const next=draft.getSceneSnapshot();if(next){const ids=new Set(rows.map(r=>r.id));const added=sceneRows(next).filter(r=>!ids.has(r.id));const first=added.find(r=>r.kind===row?.kind)??added[0];if(first)setSelected(first.id);else if(selected&&!sceneRows(next).some(r=>r.id===selected))setSelected(null);}}catch(cause){setFailure(cause instanceof Error?cause:new Error(String(cause)));}};
  const select=(id:string)=>{if(id===selected)return;if(!draft.guard())return;setSelected(id);setBinding('header');setTool('select');setFailure(null);const actor=state?.actors.findIndex(actor=>actor.id===id)??-1;if(actor>=0){const entry=state!.events.graph.entries.find(entry=>entry.actor===actor);if(entry)onScript(actor,entry.event,entry.pc);}};
  const apply=()=>{
    if(!row||!state||validation||!draft.inlineDirty)return;
    if(newFace&&row.kind==='triangle'){const value=state.collision_entities.layers.find(layer=>layer.id===row.parent)!.triangles[row.index].value;perform({kind:'add_collision_triangle',layer:row.parent!,vertices:faceVertices,attribute:value.attribute,height_bump_raw:value.height_bump_raw});return;}
    if(placement){const owner=placement.owners.find(owner=>owner.actor===row.id)!;perform({kind:'set_placement',change:{node:placement.node,actor:owner.actor,event:owner.event,position:position.map((v,i)=>axes[i]?v:null) as [number|null,number|null,number|null],layer:null}});}
    else if(row.kind==='actor'||row.kind==='descriptor')perform({kind:'set_descriptor',id:row.id,position,rotation});
    else if(row.kind==='zone')perform({kind:'set_zone',id:row.id,corners});
    else if(row.kind==='vertex')perform({kind:'set_collision_vertex',id:row.id,position});
    else if(row.kind==='triangle'){
      const layer=state.collision_entities.layers.find(layer=>layer.id===row.parent)!,face=layer.triangles[row.index];
      perform({kind:'batch',actions:face.value.vertices.map(id=>({kind:'set_collision_vertex',id,position:layer.vertices.find(vertex=>vertex.id===id)!.value.slice(0,3).map((v,i)=>v+position[i]) as SceneVector}))});
    }else{const value=state.entrances[row.index];perform({kind:'set_entrance',value:{...value,x:position[0],z:position[2]}});}
  };
  const preview=(value:SceneTransform)=>{
    if(!row||draft.pending||draft.inlineDirty)return;
    const point=fromView(value.position);
    if(row.kind==='zone'){
      const delta=point.map((v,i)=>Math.round(v-row.position[i]));setCorners(state!.zones[row.index].value.corners.map(corner=>corner.map((v,i)=>v+delta[i])) as SceneCorners);
    }else if(row.kind==='triangle')setPosition(point.map((v,i)=>Math.round(v-row.position[i])) as SceneVector);
    else setPosition(point.map((v,i)=>axes[i]?Math.round(v):position[i]) as SceneVector);
    if(canRotate)setRotation(value.rotation.map((v,i)=>{const units=Math.round(v*(i===1?1:-1)*2048/Math.PI);return (rotation[i]&0xf000)+((units%4096+4096)%4096);}) as SceneVector);
    draft.dirty(true);
  };
  const crud=(kind:'add'|'duplicate'|'delete')=>{
    if(!row||!state||!draft.guard())return;
    if(row.kind==='actor')perform(kind==='add'?{kind:'add_actor',template:row.id,position:state.descriptors[row.index].value.position,rotation:state.descriptors[row.index].value.rotation}:{kind:kind==='duplicate'?'duplicate_actor':'delete_actor',id:row.id});
    else if(row.kind==='descriptor')perform(kind==='add'?{kind:'add_instance',template:row.id,model:state.descriptors[row.index].value.model,position:state.descriptors[row.index].value.position,rotation:state.descriptors[row.index].value.rotation}:{kind:kind==='duplicate'?'duplicate_instance':'delete_instance',id:row.id});
    else if(row.kind==='zone')perform(kind==='add'?{kind:'add_zone',corners:state.zones[row.index].value.corners}:{kind:kind==='duplicate'?'duplicate_zone':'delete_zone',id:row.id});
    else if(row.kind==='vertex')perform(kind==='add'?{kind:'add_collision_vertex',layer:row.parent!,position:row.position}:{kind:kind==='duplicate'?'duplicate_collision_vertex':'delete_collision_vertex',id:row.id});
    else if(row.kind==='triangle'){
      if(kind==='add'){setNewFace(true);draft.dirty(true);}
      else perform({kind:kind==='duplicate'?'duplicate_collision_triangle':'delete_collision_triangle',id:row.id});
    }else setFailure(new Error('Entrance structural CRUD is unavailable: the original format has no count field and incoming saved/variable references are not closed. Existing-selector transforms remain supported.'));
  };
  const changePosition=(axis:number,value:number)=>{setPosition(previous=>previous.map((v,i)=>i===axis?value:v) as SceneVector);draft.dirty(true);};
  const changeRotation=(axis:number,value:number)=>{setRotation(previous=>previous.map((v,i)=>i===axis?value:v) as SceneVector);draft.dirty(true);};
  return{state,beginTransform:()=>{const token=draft.capture(),owner={selected,binding};return()=>{draft.assertUnchanged(token);if(selectionRef.current.selected!==owner.selected||selectionRef.current.binding!==owner.binding)throw new Error('Scene selection/source binding changed during the transform; fields were not adopted');};},invalidPosition,invalidRotation,invalidCorners,newFace,faceVertices,setFaceVertex:(axis:number,id:string)=>{setFaceVertices(previous=>previous.map((v,i)=>i===axis?id:v) as [string,string,string]);draft.dirty(true);},rows,row,selected,select,binding,placement,bindings,setBinding:(value:string)=>{if(draft.guard()){setBinding(value);setTool('select');}},tool,setTool,frame,frameSelection:()=>setFrame(v=>v+1),filter,setFilter,visible,setVisible,position,rotation,corners,changePosition,changeRotation,changeCorner:(corner:number,axis:number,value:number)=>{setCorners(previous=>previous.map((p,n)=>n===corner?p.map((v,i)=>i===axis?value:v):p) as SceneCorners);draft.dirty(true);},axes,canRotate,validation,failure,apply,preview,crud,syncScript:(selection:{actor:number;event:number;pc:number|null})=>{const actor=state?.actors[selection.actor];if(!actor)return;setSelected(actor.id);const node=selection.pc===null?null:Object.entries(state!.events.bindings).find(([,pc])=>pc===selection.pc)?.[0];const ins=state!.events.graph.instructions.find(i=>i.pc===selection.pc);setBinding(node&&ins&&handlers.has(ins.spec.handler)?node:'header');},openScript:()=>{if(!placement||!state||!row)return;const actor=state.actors.findIndex(a=>a.id===row.id),owner=placement.owners.find(o=>o.actor===row.id);if(owner)onScript(actor,owner.event,placement.pc,true);}};
}
export type SceneEditor=ReturnType<typeof useSceneEditor>;
