import { Navigate, Route, Routes, Link, NavLink } from "react-router-dom";
import { useEffect, useState, type ReactNode } from "react";
import {
  api,
  getContextLabel,
  getContextUser,
  isReadOnlyContext,
  setContextUser,
} from "./api/client";
import type { CollaborationContext } from "./api/types";
import { AuthProvider, useAuth } from "./auth/AuthContext";
import { LoginPage } from "./pages/LoginPage";
import { RegisterPage } from "./pages/RegisterPage";
import { ProfilePage } from "./pages/ProfilePage";
import { AccountsPage } from "./pages/AccountsPage";
import { IncomePage } from "./pages/IncomePage";
import { SpendingPage } from "./pages/SpendingPage";
import { LifeEventsPage } from "./pages/LifeEventsPage";
import { AssumptionsPage } from "./pages/AssumptionsPage";
import { ProjectionPage } from "./pages/ProjectionPage";
import { PlansPage } from "./pages/PlansPage";
import { ComparisonPage } from "./pages/ComparisonPage";
import { QuarterlyReviewPage } from "./pages/QuarterlyReviewPage";
import { TaxDocumentsPage } from "./pages/TaxDocumentsPage";
import { SocialSecurityPage } from "./pages/SocialSecurityPage";
import { InsightsPage } from "./pages/InsightsPage";
import { CollaborationPage } from "./pages/CollaborationPage";

function RequireAuth({ children }: { children: ReactNode }) {
  const { user, loading } = useAuth();
  if (loading) return <p className="muted center">Loading…</p>;
  if (!user) return <Navigate to="/login" replace />;
  return <>{children}</>;
}

function RedirectIfAuthed({ children }: { children: ReactNode }) {
  const { user, loading } = useAuth();
  if (loading) return <p className="muted center">Loading…</p>;
  if (user) return <Navigate to="/" replace />;
  return <>{children}</>;
}

function ContextSwitcher({ selfId }: { selfId: string }) {
  const [contexts, setContexts] = useState<CollaborationContext[]>([]);

  useEffect(() => {
    api
      .listCollaborationContexts()
      .then(setContexts)
      .catch(() => setContexts([]));
  }, []);

  if (contexts.length <= 1) return null;

  const active = getContextUser() ?? selfId;

  return (
    <select
      className="input context-switcher"
      value={active}
      onChange={(e) => {
        const next = e.target.value;
        const chosen = contexts.find((c) => c.user_id === next);
        setContextUser(next === selfId ? null : next, chosen?.role, chosen?.label);
        window.location.reload();
      }}
    >
      {contexts.map((ctx) => (
        <option key={ctx.user_id} value={ctx.user_id}>
          {ctx.label}
        </option>
      ))}
    </select>
  );
}

function Header() {
  const { user, logout } = useAuth();
  if (!user) return null;
  return (
    <header className="app-header">
      <div className="header-left">
        <Link to="/" className="brand">
          Lifetime Income Planner
        </Link>
        <nav className="main-nav">
          <NavLink to="/profile" className={({ isActive }) => (isActive ? "active" : "")}>
            Profile
          </NavLink>
          <NavLink to="/accounts" className={({ isActive }) => (isActive ? "active" : "")}>
            Accounts
          </NavLink>
          <NavLink to="/income" className={({ isActive }) => (isActive ? "active" : "")}>
            Income
          </NavLink>
          <NavLink to="/spending" className={({ isActive }) => (isActive ? "active" : "")}>
            Spending
          </NavLink>
          <NavLink to="/life-events" className={({ isActive }) => (isActive ? "active" : "")}>
            Life events
          </NavLink>
          <NavLink to="/assumptions" className={({ isActive }) => (isActive ? "active" : "")}>
            Assumptions
          </NavLink>
          <NavLink to="/projection" className={({ isActive }) => (isActive ? "active" : "")}>
            Plan
          </NavLink>
          <NavLink to="/plans" className={({ isActive }) => (isActive ? "active" : "")}>
            Saved
          </NavLink>
          <NavLink to="/compare" className={({ isActive }) => (isActive ? "active" : "")}>
            Compare
          </NavLink>
          <NavLink to="/quarterly-review" className={({ isActive }) => (isActive ? "active" : "")}>
            Review
          </NavLink>
          <NavLink to="/insights" className={({ isActive }) => (isActive ? "active" : "")}>
            Insights
          </NavLink>
          <NavLink to="/tax-documents" className={({ isActive }) => (isActive ? "active" : "")}>
            Tax docs
          </NavLink>
          <NavLink to="/social-security" className={({ isActive }) => (isActive ? "active" : "")}>
            Social Security
          </NavLink>
          <NavLink to="/collaboration" className={({ isActive }) => (isActive ? "active" : "")}>
            Collaboration
          </NavLink>
        </nav>
      </div>
      <nav className="header-right">
        <ContextSwitcher selfId={user.id} />
        <span className="muted">{user.email}</span>
        <button className="btn btn-ghost" onClick={logout}>
          Log out
        </button>
      </nav>
    </header>
  );
}

function ReadOnlyBanner() {
  const { user } = useAuth();
  if (!user || !isReadOnlyContext()) return null;
  const label = getContextLabel() ?? "this plan";
  return (
    <div className="alert alert-error read-only-banner">
      Viewing {label} as an advisor — read-only. Changes won't be saved.
    </div>
  );
}

function Shell() {
  return (
    <>
      <Header />
      <ReadOnlyBanner />
      <main className="app-main">
        <Routes>
          <Route
            path="/login"
            element={
              <RedirectIfAuthed>
                <LoginPage />
              </RedirectIfAuthed>
            }
          />
          <Route
            path="/register"
            element={
              <RedirectIfAuthed>
                <RegisterPage />
              </RedirectIfAuthed>
            }
          />
          <Route
            path="/"
            element={
              <RequireAuth>
                <ProfilePage />
              </RequireAuth>
            }
          />
          <Route
            path="/profile"
            element={
              <RequireAuth>
                <ProfilePage />
              </RequireAuth>
            }
          />
          <Route
            path="/accounts"
            element={
              <RequireAuth>
                <AccountsPage />
              </RequireAuth>
            }
          />
          <Route
            path="/income"
            element={
              <RequireAuth>
                <IncomePage />
              </RequireAuth>
            }
          />
          <Route
            path="/spending"
            element={
              <RequireAuth>
                <SpendingPage />
              </RequireAuth>
            }
          />
          <Route
            path="/life-events"
            element={
              <RequireAuth>
                <LifeEventsPage />
              </RequireAuth>
            }
          />
          <Route
            path="/assumptions"
            element={
              <RequireAuth>
                <AssumptionsPage />
              </RequireAuth>
            }
          />
          <Route
            path="/projection"
            element={
              <RequireAuth>
                <ProjectionPage />
              </RequireAuth>
            }
          />
          <Route
            path="/plans"
            element={
              <RequireAuth>
                <PlansPage />
              </RequireAuth>
            }
          />
          <Route
            path="/compare"
            element={
              <RequireAuth>
                <ComparisonPage />
              </RequireAuth>
            }
          />
          <Route
            path="/quarterly-review"
            element={
              <RequireAuth>
                <QuarterlyReviewPage />
              </RequireAuth>
            }
          />
          <Route
            path="/insights"
            element={
              <RequireAuth>
                <InsightsPage />
              </RequireAuth>
            }
          />
          <Route
            path="/tax-documents"
            element={
              <RequireAuth>
                <TaxDocumentsPage />
              </RequireAuth>
            }
          />
          <Route
            path="/social-security"
            element={
              <RequireAuth>
                <SocialSecurityPage />
              </RequireAuth>
            }
          />
          <Route
            path="/collaboration"
            element={
              <RequireAuth>
                <CollaborationPage />
              </RequireAuth>
            }
          />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </>
  );
}

export function App() {
  return (
    <AuthProvider>
      <Shell />
    </AuthProvider>
  );
}
