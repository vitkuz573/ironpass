import createClient from "openapi-fetch";
import type { paths } from "./schema";

const API_BASE_URL =
  typeof process !== "undefined" && process.env.NEXT_PUBLIC_API_URL
    ? process.env.NEXT_PUBLIC_API_URL
    : "http://127.0.0.1:8080";

export const api = createClient<paths>({ baseUrl: API_BASE_URL });

export type ApiClient = typeof api;
