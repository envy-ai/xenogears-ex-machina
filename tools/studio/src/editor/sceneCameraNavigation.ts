/** Camera-only navigation for the shared root-space Scene viewport. */
import * as THREE from 'three';

export function sceneCameraAxes(keys:ReadonlySet<string>):{forward:number;right:number} {
  return {
    forward:Number(keys.has('KeyW')||keys.has('ArrowUp'))-Number(keys.has('KeyS')||keys.has('ArrowDown')),
    right:Number(keys.has('KeyD')||keys.has('ArrowRight'))-Number(keys.has('KeyA')||keys.has('ArrowLeft')),
  };
}
export function isSceneCameraKey(code:string):boolean {
  return ['KeyW','KeyA','KeyS','KeyD','KeyQ','KeyE','ArrowUp','ArrowDown','ArrowLeft','ArrowRight'].includes(code);
}
/** Move the camera AND pivot, retaining orientation and their separation even
 * when wheel zoom has brought the camera very close to its old orbit target. */
export function moveSceneCamera(camera:THREE.PerspectiveCamera,target:THREE.Vector3,forward:number,right:number,distance:number):void {
  if(!Number.isFinite(distance)||distance<0||!Number.isFinite(forward)||!Number.isFinite(right))throw new Error('Scene camera movement must have finite axes and a nonnegative distance');
  if((forward===0&&right===0)||distance===0)return;
  const direction=camera.getWorldDirection(new THREE.Vector3()).multiplyScalar(forward);
  const side=new THREE.Vector3().setFromMatrixColumn(camera.matrixWorld,0).normalize();
  direction.addScaledVector(side,right).normalize().multiplyScalar(distance);
  camera.position.add(direction);target.add(direction);
}

export function sceneCameraTurn(keys:ReadonlySet<string>):number {
  return Number(keys.has('KeyQ'))-Number(keys.has('KeyE'));
}
/** Turn in place about the camera's local up axis, not around the orbit pivot.
 * Positive angles look left; preserve the distance to the new forward target. */
export function turnSceneCamera(camera:THREE.PerspectiveCamera,target:THREE.Vector3,radians:number):void {
  if(!Number.isFinite(radians))throw new Error('Scene camera turn must have a finite angle');
  if(radians===0)return;
  const distance=camera.position.distanceTo(target);
  camera.rotateY(radians);
  target.copy(camera.getWorldDirection(new THREE.Vector3()).multiplyScalar(distance).add(camera.position));
}
