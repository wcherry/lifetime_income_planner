import type {
  Account,
  AccountRequest,
  Assumptions,
  AssumptionsRequest,
  AuthResponse,
  IncomeRequest,
  IncomeSource,
  LifeEvent,
  LifeEventRequest,
  Plan,
  Profile,
  Projection,
  SavePlanRequest,
  SpendingItem,
  SpendingRequest,
  UpsertProfileRequest,
  User,
} from "./types";

const TOKEN_KEY = "lip_token";

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token);
}

export function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY);
}

/** Thrown for any non-2xx API response; `message` is the server's error text. */
export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = "ApiError";
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = {};
  if (body !== undefined) headers["Content-Type"] = "application/json";
  const token = getToken();
  if (token) headers["Authorization"] = `Bearer ${token}`;

  const res = await fetch(`/api${path}`, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });

  if (!res.ok) {
    let message = `Request failed (${res.status})`;
    try {
      const data = await res.json();
      if (data && typeof data.error === "string") message = data.error;
    } catch {
      // Non-JSON error body; keep the default message.
    }
    throw new ApiError(res.status, message);
  }

  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

/**
 * Download the multi-year tax-summary CSV (feature 8) and trigger a browser
 * save. A bespoke fetch (rather than `request<T>`) since the response is a
 * file, not JSON, and still needs the auth header a plain `<a href>` can't
 * carry.
 */
async function downloadTaxSummaryCsv(): Promise<void> {
  const headers: Record<string, string> = {};
  const token = getToken();
  if (token) headers["Authorization"] = `Bearer ${token}`;

  const res = await fetch("/api/reports/tax-summary.csv", { headers });
  if (!res.ok) {
    let message = `Failed to download tax report (${res.status})`;
    try {
      const data = await res.json();
      if (data && typeof data.error === "string") message = data.error;
    } catch {
      // Non-JSON error body; keep the default message.
    }
    throw new ApiError(res.status, message);
  }

  const blob = await res.blob();
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = "tax-summary.csv";
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

export const api = {
  register: (email: string, password: string) =>
    request<AuthResponse>("POST", "/auth/register", { email, password }),

  login: (email: string, password: string) =>
    request<AuthResponse>("POST", "/auth/login", { email, password }),

  me: () => request<User>("GET", "/auth/me"),

  getProfile: () => request<Profile>("GET", "/profile"),

  saveProfile: (payload: UpsertProfileRequest) => request<Profile>("PUT", "/profile", payload),

  listAccounts: () => request<Account[]>("GET", "/accounts"),

  createAccount: (payload: AccountRequest) => request<Account>("POST", "/accounts", payload),

  updateAccount: (id: string, payload: AccountRequest) =>
    request<Account>("PUT", `/accounts/${id}`, payload),

  deleteAccount: (id: string) => request<void>("DELETE", `/accounts/${id}`),

  listSpending: () => request<SpendingItem[]>("GET", "/spending"),

  createSpending: (payload: SpendingRequest) => request<SpendingItem>("POST", "/spending", payload),

  updateSpending: (id: string, payload: SpendingRequest) =>
    request<SpendingItem>("PUT", `/spending/${id}`, payload),

  deleteSpending: (id: string) => request<void>("DELETE", `/spending/${id}`),

  listIncome: () => request<IncomeSource[]>("GET", "/income"),

  createIncome: (payload: IncomeRequest) => request<IncomeSource>("POST", "/income", payload),

  updateIncome: (id: string, payload: IncomeRequest) =>
    request<IncomeSource>("PUT", `/income/${id}`, payload),

  deleteIncome: (id: string) => request<void>("DELETE", `/income/${id}`),

  listLifeEvents: () => request<LifeEvent[]>("GET", "/life-events"),

  createLifeEvent: (payload: LifeEventRequest) =>
    request<LifeEvent>("POST", "/life-events", payload),

  updateLifeEvent: (id: string, payload: LifeEventRequest) =>
    request<LifeEvent>("PUT", `/life-events/${id}`, payload),

  deleteLifeEvent: (id: string) => request<void>("DELETE", `/life-events/${id}`),

  getAssumptions: () => request<Assumptions>("GET", "/assumptions"),

  saveAssumptions: (payload: AssumptionsRequest) =>
    request<Assumptions>("PUT", "/assumptions", payload),

  getProjection: () => request<Projection>("GET", "/projection"),

  downloadTaxSummaryCsv,

  listPlans: () => request<Plan[]>("GET", "/plans"),

  savePlan: (payload: SavePlanRequest) => request<Plan>("POST", "/plans", payload),

  renamePlan: (id: string, payload: SavePlanRequest) =>
    request<Plan>("PUT", `/plans/${id}`, payload),

  loadPlan: (id: string) => request<Plan>("POST", `/plans/${id}/load`),

  deletePlan: (id: string) => request<void>("DELETE", `/plans/${id}`),
};
