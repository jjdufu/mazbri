export const FALLBACK_CORE_CONFIG_NAMES = [
  "global",
  "mazbri-cert",
  "mazbri-mgmt",
  "mazbri-auth",
  "mazbri-ratelimit",
  "mazbri-cache",
  "mazbri-billing",
  "mazbri-cbpro",
  "mazbri-cbpro-ext",
  "mazbri-bypass",
  "mazbri-gateway",
  "mazbri-analytics",
] as const;

export function isCoreProtectedConfigName(
  serviceName: string,
  protectedSet: ReadonlySet<string> = new Set(FALLBACK_CORE_CONFIG_NAMES),
): boolean {
  return protectedSet.has(serviceName.trim());
}

export interface GatewayCorsConfig {
  allowed_origins: string[];
  allow_credentials: boolean;
}

export function parseGatewayCors(tomlContent: string): GatewayCorsConfig {
  const corsSectionRegex = /\[cors\][\s\S]*?(?=\n\s*\[[a-zA-Z_]|$)/;
  const match = tomlContent.match(corsSectionRegex);
  if (!match) {
    return {
      allowed_origins: [],
      allow_credentials: true,
    };
  }
  const corsSection = match[0];

  let allowed_origins: string[] = [];
  const originsMatch = corsSection.match(/allowed_origins\s*=\s*\[([^\]]*)\]/);
  if (originsMatch) {
    const rawOrigins = originsMatch[1];
    allowed_origins = rawOrigins
      .split(",")
      .map((s) => s.trim().replace(/^["']|["']$/g, ""))
      .filter((s) => s.length > 0);
  }

  let allow_credentials = true;
  const credentialsMatch = corsSection.match(
    /allow_credentials\s*=\s*(true|false)/,
  );
  if (credentialsMatch) {
    allow_credentials = credentialsMatch[1] === "true";
  }

  return { allowed_origins, allow_credentials };
}

export function serializeGatewayCors(
  tomlContent: string,
  allowedOrigins: string[],
  allowCredentials: boolean,
): string {
  const corsSectionRegex = /\[cors\][\s\S]*?(?=\n\s*\[[a-zA-Z_]|$)/;
  const match = tomlContent.match(corsSectionRegex);

  const formattedOrigins = `allowed_origins = [${allowedOrigins.map((s) => `"${s}"`).join(", ")}]`;
  const formattedCredentials = `allow_credentials = ${allowCredentials}`;
  const newCorsSection = `[cors]\n${formattedOrigins}\n${formattedCredentials}\n\n`;

  if (match) {
    return tomlContent.replace(match[0], newCorsSection);
  }
  const trimmed = tomlContent.trim();
  if (trimmed.length === 0) {
    return newCorsSection;
  }
  return `${trimmed}\n\n${newCorsSection}`;
}
