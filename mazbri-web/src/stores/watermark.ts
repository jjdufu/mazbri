import { create } from "zustand";
import { fetchSystemSettings } from "@/api/settings";
import type { SystemSettings } from "@/types/api";

export interface WatermarkState {
  visible: boolean;
  siteName: string;
  siteUrl: string;
  tenantGatewayUrl: string;
  loaded: boolean;
  setVisible: (visible: boolean) => void;
  toggleVisible: () => void;
  applySettings: (settings: SystemSettings) => void;
  loadFromServer: (force?: boolean) => Promise<SystemSettings | null>;
  reset: () => void;
}

export const useWatermarkStore = create<WatermarkState>((set, get) => ({
  visible: false,
  siteName: "",
  siteUrl: "",
  tenantGatewayUrl: "",
  loaded: false,
  setVisible: (visible: boolean) => {
    set({ visible });
  },
  toggleVisible: () => {
    set((state) => ({ visible: !state.visible }));
  },
  applySettings: (settings: SystemSettings) => {
    set((state) => ({
      visible:
        typeof settings.watermark_visible === "boolean"
          ? settings.watermark_visible
          : state.visible,
      siteName:
        typeof settings.site_name === "string"
          ? settings.site_name
          : state.siteName,
      siteUrl:
        typeof settings.site_url === "string"
          ? settings.site_url
          : state.siteUrl,
      tenantGatewayUrl:
        typeof settings.tenant_gateway_url === "string"
          ? settings.tenant_gateway_url
          : state.tenantGatewayUrl,
      loaded: true,
    }));
  },
  loadFromServer: async (force = false) => {
    if (!force && get().loaded) {
      return null;
    }
    const settings = await fetchSystemSettings(force);
    get().applySettings(settings);
    return settings;
  },
  reset: () => {
    set({
      visible: false,
      siteName: "",
      siteUrl: "",
      tenantGatewayUrl: "",
      loaded: false,
    });
  },
}));
