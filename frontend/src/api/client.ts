import type {
  Account,
  AccountRequest,
  Assumptions,
  AssumptionsRequest,
  AuthResponse,
  ClonePlanRequest,
  Collaborator,
  CollaborationContext,
  CompareScenariosRequest,
  CompleteQuarterlyReviewRequest,
  ImportSocialSecurityEstimateRequest,
  ImportTaxDocumentRequest,
  IncomeRequest,
  IncomeSource,
  Insight,
  Invitation,
  InviteCollaboratorRequest,
  LifeEvent,
  LifeEventRequest,
  MonteCarloRequest,
  MonteCarloResult,
  OptimizeRequest,
  OptimizeResponse,
  Plan,
  PlaidItem,
  PlaidSandboxConnectRequest,
  PlaidSyncResponse,
  PlanVersion,
  Profile,
  Projection,
  QuarterlyReview,
  QuarterlyReviewOverview,
  SavePlanRequest,
  ScenarioComparison,
  SocialSecurityEstimate,
  SpendingItem,
  SpendingRequest,
  TaxDocument,
  TaxDocumentYearSummary,
  UpsertProfileRequest,
  User,
  WhatIfRequest,
} from "./types";

const TOKEN_KEY = "lip_token";
const CONTEXT_KEY = "lip_context_user";
const CONTEXT_ROLE_KEY = "lip_context_role";
const CONTEXT_LABEL_KEY = "lip_context_label";

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token);
}

export function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY);
}

/**
 * Collaboration (Phase 6, feature 7): the id of the owner whose data the
 * caller is currently acting as, or `null` to act as themselves. Sent as the
 * `X-Context-User` header on every request.
 */
export function getContextUser(): string | null {
  return localStorage.getItem(CONTEXT_KEY);
}

/**
 * `role`/`label` mirror the chosen `CollaborationContext` so the UI can show
 * a "read-only" notice without an extra round trip. The backend is the real
 * enforcement point (`AuthUser` rejects advisor writes regardless of what's
 * cached here); this is purely to keep the UI from offering doomed actions.
 */
export function setContextUser(userId: string | null, role?: string, label?: string): void {
  if (userId) {
    localStorage.setItem(CONTEXT_KEY, userId);
    if (role) localStorage.setItem(CONTEXT_ROLE_KEY, role);
    if (label) localStorage.setItem(CONTEXT_LABEL_KEY, label);
  } else {
    localStorage.removeItem(CONTEXT_KEY);
    localStorage.removeItem(CONTEXT_ROLE_KEY);
    localStorage.removeItem(CONTEXT_LABEL_KEY);
  }
}

export function getContextRole(): string | null {
  return localStorage.getItem(CONTEXT_ROLE_KEY);
}

export function getContextLabel(): string | null {
  return localStorage.getItem(CONTEXT_LABEL_KEY);
}

/** True when the active context is a read-only advisor grant. */
export function isReadOnlyContext(): boolean {
  return getContextUser() !== null && getContextRole() === "advisor";
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
  const contextUser = getContextUser();
  if (contextUser) headers["X-Context-User"] = contextUser;

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
  const contextUser = getContextUser();
  if (contextUser) headers["X-Context-User"] = contextUser;

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

  runWhatIf: (payload: WhatIfRequest) =>
    request<Projection>("POST", "/projection/what-if", payload),

  optimizeProjection: (payload: OptimizeRequest) =>
    request<OptimizeResponse>("POST", "/projection/optimize", payload),

  runMonteCarlo: (payload: MonteCarloRequest) =>
    request<MonteCarloResult>("POST", "/monte-carlo", payload),

  downloadTaxSummaryCsv,

  listPlans: () => request<Plan[]>("GET", "/plans"),

  compareScenarios: (payload: CompareScenariosRequest) =>
    request<ScenarioComparison[]>("POST", "/plans/compare", payload),

  savePlan: (payload: SavePlanRequest) => request<Plan>("POST", "/plans", payload),

  clonePlan: (id: string, payload: ClonePlanRequest = {}) =>
    request<Plan>("POST", `/plans/${id}/clone`, payload),

  updatePlanSnapshot: (id: string) => request<Plan>("POST", `/plans/${id}/versions`),

  listPlanVersions: (id: string) => request<PlanVersion[]>("GET", `/plans/${id}/versions`),

  restorePlanVersion: (id: string, versionId: string) =>
    request<Plan>("POST", `/plans/${id}/versions/${versionId}/restore`),

  renamePlan: (id: string, payload: SavePlanRequest) =>
    request<Plan>("PUT", `/plans/${id}`, payload),

  loadPlan: (id: string) => request<Plan>("POST", `/plans/${id}/load`),

  deletePlan: (id: string) => request<void>("DELETE", `/plans/${id}`),

  getQuarterlyReviews: () => request<QuarterlyReviewOverview>("GET", "/quarterly-reviews"),

  completeQuarterlyReview: (
    year: number,
    quarter: number,
    payload: CompleteQuarterlyReviewRequest,
  ) => request<QuarterlyReview>("POST", `/quarterly-reviews/${year}/${quarter}/complete`, payload),

  // --- Phase 6: financial account aggregation (Plaid) ---

  connectPlaidSandbox: (payload: PlaidSandboxConnectRequest) =>
    request<PlaidItem>("POST", "/plaid/sandbox-connect", payload),

  listPlaidItems: () => request<PlaidItem[]>("GET", "/plaid/items"),

  syncPlaidItem: (id: string) => request<PlaidSyncResponse>("POST", `/plaid/items/${id}/sync`),

  deletePlaidItem: (id: string) => request<void>("DELETE", `/plaid/items/${id}`),

  // --- Phase 6: tax form imports ---

  importTaxDocument: (payload: ImportTaxDocumentRequest) =>
    request<TaxDocument>("POST", "/tax-documents/import", payload),

  listTaxDocuments: (year?: number) =>
    request<TaxDocument[]>("GET", year ? `/tax-documents?year=${year}` : "/tax-documents"),

  getTaxDocumentYearSummary: (year: number) =>
    request<TaxDocumentYearSummary>("GET", `/tax-documents/summary/${year}`),

  deleteTaxDocument: (id: string) => request<void>("DELETE", `/tax-documents/${id}`),

  // --- Phase 6: Social Security statement import ---

  importSocialSecurityEstimate: (payload: ImportSocialSecurityEstimateRequest) =>
    request<SocialSecurityEstimate>("POST", "/social-security-estimates/import", payload),

  listSocialSecurityEstimates: () =>
    request<SocialSecurityEstimate[]>("GET", "/social-security-estimates"),

  deleteSocialSecurityEstimate: (id: string) =>
    request<void>("DELETE", `/social-security-estimates/${id}`),

  // --- Phase 6: personalized insights & anomaly detection ---

  getInsights: () => request<Insight[]>("GET", "/insights"),

  // --- Phase 6: collaboration (spouses & advisors) ---

  inviteCollaborator: (payload: InviteCollaboratorRequest) =>
    request<Collaborator>("POST", "/collaborators", payload),

  listCollaborators: () => request<Collaborator[]>("GET", "/collaborators"),

  listInvitations: () => request<Invitation[]>("GET", "/collaborators/invitations"),

  acceptInvitation: (id: string) => request<Collaborator>("POST", `/collaborators/${id}/accept`),

  declineInvitation: (id: string) => request<Collaborator>("POST", `/collaborators/${id}/decline`),

  revokeCollaborator: (id: string) => request<void>("DELETE", `/collaborators/${id}`),

  listCollaborationContexts: () =>
    request<CollaborationContext[]>("GET", "/collaborators/contexts"),
};
