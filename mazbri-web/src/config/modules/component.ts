import { defineAsyncComponent } from "vue";
import { $t } from "@/locales";
export const globalComponentsConfig: GlobalComponentConfig[] = [
  {
    name: $t("config.globalComponents.screenLock"),
    key: "screen-lock",
    component: defineAsyncComponent(
      () => import("@/components/core/layouts/art-screen-lock/index.vue"),
    ),
    enabled: true,
  },
  {
    name: $t("config.globalComponents.fireworksEffect"),
    key: "fireworks-effect",
    component: defineAsyncComponent(
      () => import("@/components/core/layouts/art-fireworks-effect/index.vue"),
    ),
    enabled: true,
  },
  {
    name: $t("config.globalComponents.watermarkEffect"),
    key: "watermark",
    component: defineAsyncComponent(
      () => import("@/components/core/others/art-watermark/index.vue"),
    ),
    enabled: true,
  },
];
export interface GlobalComponentConfig {
  name: string;
  key: string;
  component: any;
  enabled?: boolean;
  description?: string;
}
export const getEnabledGlobalComponents = () => {
  return globalComponentsConfig.filter((config) => config.enabled !== false);
};
export const getGlobalComponentByKey = (key: string) => {
  return globalComponentsConfig.find((config) => config.key === key);
};
