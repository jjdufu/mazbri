import mitt, { type Emitter } from "mitt";
type Events = {
  triggerFireworks: string | undefined;
  openLockScreen: void;
};
const mittBus: Emitter<Events> = mitt<Events>();
export default mittBus;
