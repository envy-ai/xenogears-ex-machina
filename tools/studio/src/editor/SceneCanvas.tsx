/** Shared Three.js viewport; caller owns authored data, validation and history. */
import { useEffect, useId, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { TransformControls } from 'three/addons/controls/TransformControls.js';
import { isSceneCameraKey, moveSceneCamera, sceneCameraAxes, sceneCameraTurn, turnSceneCamera } from './sceneCameraNavigation';
import '../styles/scene-canvas.scss';
export type SceneTool='select'|'move'|'rotate';
export interface SceneCanvasModel { group:THREE.Group; objects:Map<string,THREE.Object3D>; frameBounds(object?:THREE.Object3D):THREE.Box3; dispose():void }
export interface SceneTransform {position:[number,number,number];rotation:[number,number,number]}
export interface SceneCanvasProps {
  model:SceneCanvasModel;selected:string|null;tool:SceneTool;frame:number;disabled:boolean;
  axes:[boolean,boolean,boolean];onSelect(id:string):void;
  onBeginTransform():()=>void;onTransform(value:SceneTransform):void;onError(error:unknown):void;
}
export function SceneCanvas(props:SceneCanvasProps){
  const helpId=useId();
  const host=useRef<HTMLDivElement>(null),latest=useRef(props),framedRequest=useRef(props.frame);latest.current=props;
  const engine=useRef<{scene:THREE.Scene;camera:THREE.PerspectiveCamera;orbit:OrbitControls;transform:TransformControls;selection:{object:THREE.Object3D|null;proxy:THREE.Object3D};frame(object?:THREE.Object3D):void}|null>(null);
  const [error,setError]=useState<Error|null>(null),[attempt,setAttempt]=useState(0);
  useEffect(()=>{
    const container=host.current!;let renderer:THREE.WebGLRenderer|undefined;let animation=0;let resize:ResizeObserver|undefined;
    let orbit:OrbitControls|undefined,transform:TransformControls|undefined;
    let removeNavigation=()=>{};
    const fail=(cause:unknown)=>{const error=cause instanceof Error?cause:new Error(String(cause));setError(error);try{latest.current.onError(error);}catch(reportError){setError(new AggregateError([error,reportError],'Viewport failure and its error reporter both failed; authored state is preserved'));}};
    try{
      setError(null);
      renderer=new THREE.WebGLRenderer({antialias:true,alpha:false});renderer.setPixelRatio(window.devicePixelRatio);renderer.setClearColor(0x111d24);renderer.outputColorSpace=THREE.SRGBColorSpace;
      const canvas=renderer.domElement;canvas.setAttribute('aria-label','3D field scene viewport');canvas.tabIndex=0;canvas.setAttribute('aria-describedby',helpId);container.append(canvas);
      const scene=new THREE.Scene(),camera=new THREE.PerspectiveCamera(45,1,1,1e7);camera.position.set(600,600,600);
      orbit=new OrbitControls(camera,canvas);orbit.enableDamping=false;
      const keys=new Set<string>();let fast=false,lastDraw:number|null=null,moveSpeed=600,mouseNavigating=false;
      const clearNavigation=()=>{keys.clear();fast=false;lastDraw=null;};
      const mouseStart=()=>{mouseNavigating=true;clearNavigation();};
      const mouseEnd=()=>{mouseNavigating=false;clearNavigation();};
      orbit.addEventListener('start',mouseStart);orbit.addEventListener('end',mouseEnd);
      transform=new TransformControls(camera,canvas);transform.setSpace('world');transform.setTranslationSnap(null);transform.setRotationSnap(Math.PI/2048);scene.add(transform.getHelper());const selection={object:null as THREE.Object3D|null,proxy:new THREE.Object3D()};scene.add(selection.proxy);
      transform.addEventListener('dragging-changed',event=>{clearNavigation();orbit!.enabled=!event.value;});
      let initialTransform:{position:THREE.Vector3;rotation:THREE.Euler;anchor:THREE.Vector3;validate:()=>void}|null=null;
      transform.addEventListener('mouseDown',()=>{const object=selection.object;initialTransform=null;if(!object)return;try{initialTransform={position:object.position.clone(),rotation:object.rotation.clone(),anchor:selection.proxy.position.clone(),validate:latest.current.onBeginTransform()};}catch(error){fail(error);}});
      transform.addEventListener('mouseUp',()=>{
        const object=selection.object;if(!object||!initialTransform)return;
        const translated=initialTransform.position.clone().add(selection.proxy.position).sub(initialTransform.anchor);
        const value:SceneTransform={position:translated.toArray() as [number,number,number],rotation:[selection.proxy.rotation.x,selection.proxy.rotation.y,selection.proxy.rotation.z]};
        const validate=initialTransform.validate;
        selection.proxy.position.copy(initialTransform.anchor);selection.proxy.rotation.copy(initialTransform.rotation);initialTransform=null;
        try{validate();latest.current.onTransform(value);}catch(error){fail(error);}
      });
      const frame=(object:THREE.Object3D=latest.current.model.group)=>{
        const box=latest.current.model.frameBounds(object===latest.current.model.group?undefined:object);if(box.isEmpty())return;
        const center=box.getCenter(new THREE.Vector3()),radius=box.getSize(new THREE.Vector3()).length()/2;
        const distance=Math.max(20,radius/Math.sin(camera.fov*Math.PI/360));
        clearNavigation();moveSpeed=distance*0.6;
        orbit!.target.copy(center);camera.position.copy(center).add(new THREE.Vector3(1,0.8,1).normalize().multiplyScalar(distance));camera.near=Math.max(0.1,distance/10000);camera.far=Math.max(10000,distance*20);camera.updateProjectionMatrix();orbit!.update();
      };
      engine.current={scene,camera,orbit,transform,selection,frame};
      if(import.meta.env.VITE_XEM_STUDIO_TEST==='1'){
        const visible=(object:THREE.Object3D)=>{let current:THREE.Object3D|null=object;while(current){if(!current.visible)return false;current=current.parent;}return true;};
        const project=(object:THREE.Object3D)=>{
          camera.updateMatrixWorld(true);scene.updateMatrixWorld(true);
          const box=new THREE.Box3().setFromObject(object);let point=box.isEmpty()?object.getWorldPosition(new THREE.Vector3()):box.getCenter(new THREE.Vector3());
          if(typeof object.userData.entity==='string'&&object.userData.sprite){
            let part:THREE.Sprite|null=null;object.traverse(child=>{if(!part&&child instanceof THREE.Sprite)part=child;});
            if(part){const sprite=part as THREE.Sprite,scale=sprite.getWorldScale(new THREE.Vector3());point.set((0.5-sprite.center.x)*scale.x,(0.5-sprite.center.y)*scale.y,0).applyQuaternion(camera.quaternion).add(sprite.getWorldPosition(new THREE.Vector3()));}
          }else if(typeof object.userData.entity==='string'){
            let surface:THREE.Mesh|null=null;object.traverse(child=>{if(!surface&&child instanceof THREE.Mesh&&child.geometry.getAttribute('position')?.count>=3)surface=child;});
            if(surface){const mesh=surface as THREE.Mesh,positions=mesh.geometry.getAttribute('position'),index=mesh.geometry.index;point=new THREE.Vector3();for(let i=0;i<3;i++)point.add(new THREE.Vector3().fromBufferAttribute(positions,index?index.getX(i):i));point.multiplyScalar(1/3).applyMatrix4(mesh.matrixWorld);}
          }
          point.project(camera);
          const bounds=canvas.getBoundingClientRect();return{x:bounds.left+(point.x+1)*bounds.width/2,y:bounds.top+(1-point.y)*bounds.height/2,z:point.z,visible:visible(object)&&point.x>=-1&&point.x<=1&&point.y>=-1&&point.y<=1&&point.z>=-1&&point.z<=1};
        };
        (canvas as HTMLCanvasElement&{__xemScene?:unknown}).__xemScene={
          camera:()=>({position:camera.position.toArray(),target:orbit!.target.toArray(),forward:camera.getWorldDirection(new THREE.Vector3()).toArray(),quaternion:camera.quaternion.toArray(),up:new THREE.Vector3(0,1,0).applyQuaternion(camera.quaternion).toArray(),right:new THREE.Vector3().setFromMatrixColumn(camera.matrixWorld,0).normalize().toArray()}),
          project:(id:string)=>{const object=latest.current.model.objects.get(id);if(!object)throw new Error('Scene object is not mounted');return project(object);},
          textureStats:()=>{
            let meshes=0,texturedMeshes=0,texturedTriangles=0,spriteParts=0;
            const spriteActors:unknown[]=[];
            const maps=new Set<THREE.Texture>();
            latest.current.model.group.traverse(object=>{
              if(!visible(object))return;
              if(object.userData.sprite)spriteActors.push({...object.userData.sprite});
              if(object instanceof THREE.Sprite&&object.material.map)spriteParts++;
              if(!(object instanceof THREE.Mesh))return;
              meshes++;
              const materials=Array.isArray(object.material)?object.material:[object.material];
              for(const material of materials){
                if(!(material instanceof THREE.MeshBasicMaterial)||!material.map)continue;
                texturedMeshes++;maps.add(material.map);
                texturedTriangles+=(object.geometry.index?.count??object.geometry.getAttribute('position')?.count??0)/3;
              }
            });
            return {source:latest.current.model.group.userData.source,meshes,texturedMeshes,texturedTriangles,texturePages:maps.size,spriteParts,spriteActors,map_stream:latest.current.model.group.userData.map_stream??null};
          },
          gizmo:(axis:string)=>{let handle:THREE.Object3D|null=null;transform!.getHelper().traverse(object=>{if(!handle&&object.name===axis&&object instanceof THREE.Mesh&&visible(object))handle=object;});if(!handle)throw new Error('Requested gizmo handle is not visible');return project(handle);},
        };
      }
      resize=new ResizeObserver(()=>{const w=container.clientWidth,h=container.clientHeight;if(w&&h){renderer!.setSize(w,h);camera.aspect=w/h;camera.updateProjectionMatrix();}});resize.observe(container);
      const ray=new THREE.Raycaster();let down:{x:number;y:number}|null=null;
      const pointerDown=(event:PointerEvent)=>{canvas.focus({preventScroll:true});down={x:event.clientX,y:event.clientY};};
      const pointerUp=(event:PointerEvent)=>{
        if(!down||event.button!==0||Math.hypot(down.x-event.clientX,down.y-event.clientY)>4||transform!.axis!==null||transform!.dragging)return;
        const bounds=canvas.getBoundingClientRect();ray.setFromCamera(new THREE.Vector2((event.clientX-bounds.left)/bounds.width*2-1,-(event.clientY-bounds.top)/bounds.height*2+1),camera);
        for(const hit of ray.intersectObject(latest.current.model.group,true)){
          let object:THREE.Object3D|null=hit.object;let id:string|null=Array.isArray(hit.object.userData.faceEntities)&&typeof hit.faceIndex==='number'?hit.object.userData.faceEntities[hit.faceIndex]:null,visible=true;
          while(object){visible&&=object.visible;if(typeof object.userData.entity==='string')id=object.userData.entity;object=object.parent;}
          if(visible&&id){latest.current.onSelect(id);break;}
        }
      };
      const contextLost=(event:Event)=>{event.preventDefault();mouseEnd();cancelAnimationFrame(animation);fail(new Error('The scene WebGL context was lost; authored state and last-good build are preserved'));};
      canvas.addEventListener('pointerdown',pointerDown);canvas.addEventListener('pointerup',pointerUp);canvas.addEventListener('webglcontextlost',contextLost);
      const keyDown=(event:KeyboardEvent)=>{
        if(event.defaultPrevented||event.isComposing||event.altKey||event.ctrlKey||event.metaKey||document.querySelector('[aria-modal=true]')){clearNavigation();return;}
        fast=event.shiftKey;
        if(!isSceneCameraKey(event.code))return;
        event.preventDefault();
        if(!mouseNavigating&&!transform!.dragging&&orbit!.enabled)keys.add(event.code);
      };
      const keyUp=(event:KeyboardEvent)=>{keys.delete(event.code);fast=event.shiftKey;};
      const suspendForModifier=(event:KeyboardEvent)=>{if(event.altKey||event.ctrlKey||event.metaKey||event.key==='Escape')clearNavigation();};
      const visibility=()=>{if(document.hidden)mouseEnd();};
      canvas.addEventListener('keydown',keyDown);canvas.addEventListener('blur',mouseEnd);
      window.addEventListener('keyup',keyUp);window.addEventListener('keydown',suspendForModifier,true);window.addEventListener('blur',mouseEnd);document.addEventListener('visibilitychange',visibility);
      removeNavigation=()=>{clearNavigation();canvas.removeEventListener('keydown',keyDown);canvas.removeEventListener('blur',mouseEnd);window.removeEventListener('keyup',keyUp);window.removeEventListener('keydown',suspendForModifier,true);window.removeEventListener('blur',mouseEnd);document.removeEventListener('visibilitychange',visibility);orbit!.removeEventListener('start',mouseStart);orbit!.removeEventListener('end',mouseEnd);};
      const draw=(now:number)=>{try{
        const elapsed=lastDraw===null?0:(now-lastDraw)/1000;lastDraw=now;
        if(document.activeElement===canvas&&!document.hidden&&!document.querySelector('[aria-modal=true]')&&!mouseNavigating&&!transform!.dragging&&orbit!.enabled){
          const axes=sceneCameraAxes(keys),turn=sceneCameraTurn(keys),rate=(fast?3:1)*elapsed;
          if(turn)turnSceneCamera(camera,orbit!.target,turn*Math.PI/3*rate);
          if(axes.forward||axes.right||axes.up)moveSceneCamera(camera,orbit!.target,axes.forward,axes.right,moveSpeed*rate,axes.up);
          // OrbitControls.update() levels the view; keyboard navigation retains
          // exact local-axis orientation until a mouse gesture or explicit Frame.
        }else clearNavigation();
        renderer!.render(scene,camera);animation=requestAnimationFrame(draw);
      }catch(error){clearNavigation();cancelAnimationFrame(animation);fail(error);}};draw(performance.now());
      return()=>{removeNavigation();cancelAnimationFrame(animation);resize?.disconnect();canvas.removeEventListener('pointerdown',pointerDown);canvas.removeEventListener('pointerup',pointerUp);canvas.removeEventListener('webglcontextlost',contextLost);transform!.dispose();orbit!.dispose();renderer!.dispose();canvas.remove();engine.current=null;};
    }catch(error){removeNavigation();cancelAnimationFrame(animation);resize?.disconnect();transform?.dispose();orbit?.dispose();renderer?.dispose();renderer?.domElement.remove();engine.current=null;fail(error);}
  },[attempt,helpId]);
  useEffect(()=>{
    const current=engine.current;if(!current)return;current.scene.add(props.model.group);if(current.scene.userData.source!==props.model.group.userData.source){current.frame();current.scene.userData.source=props.model.group.userData.source;}
    return()=>{current.transform.detach();current.scene.remove(props.model.group);};
  },[props.model,attempt]);
  useEffect(()=>()=>props.model.dispose(),[props.model]);
  useEffect(()=>{
    const current=engine.current;if(!current)return;const object=props.selected?props.model.objects.get(props.selected):null;
    current.transform.detach();current.selection.object=object??null;
    if(object&&props.tool!=='select'&&!props.disabled&&props.axes.some(Boolean)){
      object.updateWorldMatrix(true,true);const bounds=new THREE.Box3().setFromObject(object);if(bounds.isEmpty())object.getWorldPosition(current.selection.proxy.position);else bounds.getCenter(current.selection.proxy.position);current.selection.proxy.rotation.copy(object.rotation);
      current.transform.setMode(props.tool==='move'?'translate':'rotate');current.transform.showX=props.axes[0];current.transform.showY=props.axes[1];current.transform.showZ=props.axes[2];current.transform.attach(current.selection.proxy);
    }
  },[props.model,props.selected,props.tool,props.disabled,props.axes,attempt]);
  // Mount/retry uses qualified overview bounds; only a new F/button request frames selection.
  useEffect(()=>{if(framedRequest.current===props.frame)return;framedRequest.current=props.frame;const object=props.selected?props.model.objects.get(props.selected):undefined;engine.current?.frame(object);},[props.frame]);
  return <div className="scene-canvas" ref={host}>{error&&<div className="scene-canvas__error"><pre className="editor-text editor-text--danger" role="alert">{error.stack??error.message}</pre><button className="editor-button" onClick={()=>setAttempt(value=>value+1)}>Retry 3D viewport</button></div>}<span id={helpId} className="scene-canvas__help editor-text editor-text--caption">Click viewport · WASD/arrows: dolly/strafe · Q/E: turn · Space/C or PgUp/PgDn: rise/descend · Shift: faster · Orbit: drag · Pan: right drag · Wheel: zoom · F: frame · Source axes: Y down</span></div>;
}
