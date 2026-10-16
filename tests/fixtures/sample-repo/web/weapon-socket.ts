import { Bone } from "./bone";

export class WeaponSocket {
  attachToBone(bone: Bone): void {
    console.log(bone.name);
  }
}
