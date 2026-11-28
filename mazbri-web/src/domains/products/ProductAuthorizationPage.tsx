import { useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import { PageHeaderActions } from "@/components/layout";
import { Tabs, type TabOption } from "@/components/ui";
import { useT } from "@/locales";
import { AuthorizationListTab } from "./AuthorizationListTab";
import { ProductListTab } from "./ProductListTab";

type ProductAuthorizationTab = "products" | "authorizations";

function resolveTab(raw: string | null): ProductAuthorizationTab {
  return raw === "authorizations" ? "authorizations" : "products";
}

export function ProductAuthorizationPage() {
  const { t } = useT();
  const [searchParams, setSearchParams] = useSearchParams();
  const activeTab = resolveTab(searchParams.get("tab"));
  const [loadedTabs, setLoadedTabs] = useState<Set<ProductAuthorizationTab>>(
    () => new Set([activeTab]),
  );

  useEffect(() => {
    setLoadedTabs((prev) => {
      if (prev.has(activeTab)) return prev;
      const next = new Set(prev);
      next.add(activeTab);
      return next;
    });
  }, [activeTab]);

  const handleTabChange = (next: ProductAuthorizationTab) => {
    const nextParams = new URLSearchParams(searchParams);
    nextParams.set("tab", next);
    setSearchParams(nextParams, { replace: true });
  };

  const tabItems: TabOption<ProductAuthorizationTab>[] = [
    {
      key: "products",
      label: t("menus.mazbri.productList"),
      testId: "tab-products",
    },
    {
      key: "authorizations",
      label: t("menus.mazbri.authorizationList"),
      testId: "tab-authorizations",
    },
  ];

  return (
    <div
      className="flex flex-col flex-1 min-h-0 gap-4 min-w-0"
      data-testid="product-authorization-page"
    >
      <PageHeaderActions>
        <Tabs
          variant="inline"
          items={tabItems}
          value={activeTab}
          onChange={handleTabChange}
        />
      </PageHeaderActions>

      {loadedTabs.has("products") && (
        <div
          className={
            activeTab === "products"
              ? "flex flex-col flex-1 min-h-0 min-w-0"
              : "hidden"
          }
        >
          <ProductListTab />
        </div>
      )}

      {loadedTabs.has("authorizations") && (
        <div
          className={
            activeTab === "authorizations"
              ? "flex flex-col flex-1 min-h-0 min-w-0"
              : "hidden"
          }
        >
          <AuthorizationListTab />
        </div>
      )}
    </div>
  );
}
