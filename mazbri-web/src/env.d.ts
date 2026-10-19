declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<
    Record<string, unknown>,
    Record<string, unknown>,
    any
  >;
  export default component;
}
declare module "nprogress";
declare module "crypto-js";
declare module "vue-img-cutter";
declare module "file-saver";
declare module "qrcode.vue" {
  export type Level = "L" | "M" | "Q" | "H";
  export type RenderAs = "canvas" | "svg";
  export type GradientType = "linear" | "radial";
  export interface ImageSettings {
    src: string;
    height: number;
    width: number;
    excavate: boolean;
  }
  export interface QRCodeProps {
    value: string;
    size?: number;
    level?: Level;
    background?: string;
    foreground?: string;
    renderAs?: RenderAs;
  }
  const QrcodeVue: any;
  export default QrcodeVue;
}
declare const __APP_VERSION__: string;
