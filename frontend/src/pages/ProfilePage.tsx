import { useEffect, useState, type FormEvent } from "react";
import { api, ApiError } from "../api/client";
import type {
  FilingStatus,
  MaritalStatus,
  UpsertProfileRequest,
} from "../api/types";
import { Alert, Button, Card, Field, Select, TextInput } from "../components/ui";
import { US_STATES } from "../data/states";

const MARITAL_OPTIONS: { value: MaritalStatus; label: string }[] = [
  { value: "single", label: "Single" },
  { value: "married", label: "Married" },
  { value: "widowed", label: "Widowed" },
];

const FILING_OPTIONS: { value: FilingStatus; label: string }[] = [
  { value: "single", label: "Single" },
  { value: "married_filing_jointly", label: "Married filing jointly" },
  { value: "married_filing_separately", label: "Married filing separately" },
  { value: "head_of_household", label: "Head of household" },
  { value: "qualifying_widow", label: "Qualifying widow(er)" },
];

const EMPTY: UpsertProfileRequest = {
  first_name: "",
  last_name: "",
  date_of_birth: "",
  marital_status: "single",
  filing_status: "single",
  state: "",
  retirement_date: "",
  life_expectancy: 95,
  spouse_first_name: "",
  spouse_last_name: "",
  spouse_date_of_birth: "",
  spouse_life_expectancy: null,
};

export function ProfilePage() {
  const [form, setForm] = useState<UpsertProfileRequest>(EMPTY);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  // Load the existing profile, if any, to prefill the form.
  useEffect(() => {
    let active = true;
    async function load() {
      try {
        const p = await api.getProfile();
        if (active) {
          setForm({
            first_name: p.first_name,
            last_name: p.last_name,
            date_of_birth: p.date_of_birth,
            marital_status: p.marital_status,
            filing_status: p.filing_status,
            state: p.state,
            retirement_date: p.retirement_date,
            life_expectancy: p.life_expectancy,
            spouse_first_name: p.spouse_first_name ?? "",
            spouse_last_name: p.spouse_last_name ?? "",
            spouse_date_of_birth: p.spouse_date_of_birth ?? "",
            spouse_life_expectancy: p.spouse_life_expectancy,
          });
        }
      } catch (err) {
        // A 404 simply means the profile hasn't been created yet.
        if (!(err instanceof ApiError && err.status === 404)) {
          if (active)
            setError(err instanceof Error ? err.message : "Failed to load profile");
        }
      } finally {
        if (active) setLoading(false);
      }
    }
    load();
    return () => {
      active = false;
    };
  }, []);

  function update<K extends keyof UpsertProfileRequest>(
    key: K,
    value: UpsertProfileRequest[K],
  ) {
    setForm((f) => ({ ...f, [key]: value }));
    setSaved(false);
  }

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setSaved(false);
    setSaving(true);
    try {
      const payload: UpsertProfileRequest = {
        ...form,
        life_expectancy: Number(form.life_expectancy),
        // Optional dates must be null, not "" — the API rejects an empty-string
        // date. This matters for single/widowed profiles that never touch the
        // spouse fields.
        spouse_date_of_birth: form.spouse_date_of_birth || null,
      };
      await api.saveProfile(payload);
      setSaved(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save profile");
    } finally {
      setSaving(false);
    }
  }

  if (loading) return <p className="muted">Loading your profile…</p>;

  const isMarried = form.marital_status === "married";

  return (
    <Card title="Retirement profile">
      <p className="muted">
        Tell us about yourself. These details drive every projection in your plan.
      </p>
      <form onSubmit={onSubmit}>
        {error && <Alert kind="error">{error}</Alert>}
        {saved && <Alert kind="success">Profile saved.</Alert>}

        <div className="grid-2">
          <Field label="First name" htmlFor="first_name">
            <TextInput
              id="first_name"
              value={form.first_name}
              onChange={(e) => update("first_name", e.target.value)}
              required
            />
          </Field>
          <Field label="Last name" htmlFor="last_name">
            <TextInput
              id="last_name"
              value={form.last_name}
              onChange={(e) => update("last_name", e.target.value)}
              required
            />
          </Field>
        </div>

        <div className="grid-2">
          <Field label="Date of birth" htmlFor="dob">
            <TextInput
              id="dob"
              type="date"
              value={form.date_of_birth}
              onChange={(e) => update("date_of_birth", e.target.value)}
              required
            />
          </Field>
          <Field label="State" htmlFor="state">
            <Select
              id="state"
              value={form.state}
              onChange={(e) => update("state", e.target.value)}
              required
            >
              <option value="">Select…</option>
              {US_STATES.map((s) => (
                <option key={s.code} value={s.code}>
                  {s.name}
                </option>
              ))}
            </Select>
          </Field>
        </div>

        <div className="grid-2">
          <Field label="Marital status" htmlFor="marital">
            <Select
              id="marital"
              value={form.marital_status}
              onChange={(e) =>
                update("marital_status", e.target.value as MaritalStatus)
              }
            >
              {MARITAL_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Tax filing status" htmlFor="filing">
            <Select
              id="filing"
              value={form.filing_status}
              onChange={(e) =>
                update("filing_status", e.target.value as FilingStatus)
              }
            >
              {FILING_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </Select>
          </Field>
        </div>

        <div className="grid-2">
          <Field label="Planned retirement date" htmlFor="retirement">
            <TextInput
              id="retirement"
              type="date"
              value={form.retirement_date}
              onChange={(e) => update("retirement_date", e.target.value)}
              required
            />
          </Field>
          <Field
            label="Life expectancy (age)"
            htmlFor="life"
            hint="Plan through this age."
          >
            <TextInput
              id="life"
              type="number"
              min={50}
              max={120}
              value={form.life_expectancy}
              onChange={(e) => update("life_expectancy", Number(e.target.value))}
              required
            />
          </Field>
        </div>

        {isMarried && (
          <fieldset className="spouse">
            <legend>Spouse details</legend>
            <div className="grid-2">
              <Field label="Spouse first name" htmlFor="s_first">
                <TextInput
                  id="s_first"
                  value={form.spouse_first_name ?? ""}
                  onChange={(e) => update("spouse_first_name", e.target.value)}
                  required
                />
              </Field>
              <Field label="Spouse last name" htmlFor="s_last">
                <TextInput
                  id="s_last"
                  value={form.spouse_last_name ?? ""}
                  onChange={(e) => update("spouse_last_name", e.target.value)}
                  required
                />
              </Field>
            </div>
            <div className="grid-2">
              <Field label="Spouse date of birth" htmlFor="s_dob">
                <TextInput
                  id="s_dob"
                  type="date"
                  value={form.spouse_date_of_birth ?? ""}
                  onChange={(e) => update("spouse_date_of_birth", e.target.value)}
                  required
                />
              </Field>
              <Field label="Spouse life expectancy (age)" htmlFor="s_life">
                <TextInput
                  id="s_life"
                  type="number"
                  min={50}
                  max={120}
                  value={form.spouse_life_expectancy ?? ""}
                  onChange={(e) =>
                    update(
                      "spouse_life_expectancy",
                      e.target.value === "" ? null : Number(e.target.value),
                    )
                  }
                />
              </Field>
            </div>
          </fieldset>
        )}

        <Button type="submit" disabled={saving}>
          {saving ? "Saving…" : "Save profile"}
        </Button>
      </form>
    </Card>
  );
}
