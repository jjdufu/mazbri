import { $t } from "@/locales";
const asciiArt = `
\x1b[32m${$t("common.consoleBanner.welcome")}
\x1b[0m
\x1b[36m${$t("common.consoleBanner.subtitle")}
\x1b[0m
`;
console.log(asciiArt);
