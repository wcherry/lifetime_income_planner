import { Navigate, Route, Routes, Link, NavLink } from "react-router-dom";
import type { ReactNode } from "react";
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
        </nav>
      </div>
      <nav className="header-right">
        <span className="muted">{user.email}</span>
        <button className="btn btn-ghost" onClick={logout}>
          Log out
        </button>
      </nav>
    </header>
  );
}

function Shell() {
  return (
    <>
      <Header />
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
