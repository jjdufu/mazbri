import type { RouteObject } from "react-router";
import { ForbiddenPage, NotFoundPage } from "./ExceptionPage";
import { LoginPage } from "./LoginPage";

export const authRoutes: RouteObject[] = [
  {
    path: "/auth/login",
    element: <LoginPage />,
  },
  {
    path: "/403",
    element: <ForbiddenPage />,
  },
  {
    path: "/404",
    element: <NotFoundPage />,
  },
];
