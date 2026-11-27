export class StorageConfig {
  static readonly CURRENT_VERSION = __APP_VERSION__;
  static readonly STORAGE_PREFIX = "sys-v";
  static readonly VERSION_KEY = "sys-version";
  static readonly THEME_KEY = "sys-theme";
  static readonly LAST_USER_ID_KEY = "sys-last-user-id";
  static readonly SKIP_UPGRADE_VERSION = "1.0.0";
  static readonly UPGRADE_DELAY = 1000;
  static readonly LOGOUT_DELAY = 1000;
  static generateStorageKey(
    storeId: string,
    version: string = this.CURRENT_VERSION,
  ): string {
    return `${this.STORAGE_PREFIX}${version}-${storeId}`;
  }
  static generateLegacyKey(version: string = this.CURRENT_VERSION): string {
    return `${this.STORAGE_PREFIX}${version}`;
  }
  static createKeyPattern(storeId: string): RegExp {
    return new RegExp(`^${this.STORAGE_PREFIX}[^-]+-${storeId}$`);
  }
  static createCurrentVersionPattern(): RegExp {
    return new RegExp(`^${this.STORAGE_PREFIX}${this.CURRENT_VERSION}-`);
  }
  static createVersionPattern(): RegExp {
    return new RegExp(`^${this.STORAGE_PREFIX}`);
  }
  static isCurrentVersionKey(key: string): boolean {
    return key.startsWith(`${this.STORAGE_PREFIX}${this.CURRENT_VERSION}`);
  }
  static isVersionedKey(key: string): boolean {
    return key.startsWith(this.STORAGE_PREFIX);
  }
  static extractVersionFromKey(key: string): string | null {
    const match = key.match(new RegExp(`^${this.STORAGE_PREFIX}([^-]+)`));
    return match ? match[1] : null;
  }
  static extractStoreIdFromKey(key: string): string | null {
    const match = key.match(new RegExp(`^${this.STORAGE_PREFIX}[^-]+-(.+)$`));
    return match ? match[1] : null;
  }
}
