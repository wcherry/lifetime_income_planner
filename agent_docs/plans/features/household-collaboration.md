# Household Collaboration — Implementation Plan

Branch: `feature/household-collaboration`

## What is changing and why

Add collaboration support so spouses, family members, and advisors can view and work on the same retirement plan. The initial version should focus on shared access, role-based permissions, and simple activity visibility rather than full real-time collaboration.

This feature should build on the existing collaboration-related backend surfaces already present in the repository and expand them into a user-facing household planning workflow.

## Scope

### In scope

- Invite collaborators to a plan or scenario
- Assign roles such as owner, editor, and viewer
- Restrict write access according to role
- Show shared plans in the collaborator’s dashboard
- Record basic collaboration activity for auditability

### Out of scope for v1

- Real-time co-editing
- Comment threads on every plan item
- Full document version conflict resolution
- Advanced permission inheritance across multiple households

## Layers affected

- Backend: collaboration endpoints, permission checks, invitation lifecycle
- Frontend: share UI, collaborator list, role assignment controls, shared-plan listing
- Data: new tables or extensions for invitations and role assignments
- Tests: permission and invitation flow tests

## Proposed data model

### `collaboration_invites`

```text
id                  TEXT PK
plan_id             TEXT NOT NULL
invited_by_user_id  TEXT NOT NULL
email               TEXT NOT NULL
role                TEXT NOT NULL -- owner | editor | viewer
status              TEXT NOT NULL -- pending | accepted | revoked | expired
expires_at          TIMESTAMP NULL
created_at          TIMESTAMP NOT NULL
updated_at          TIMESTAMP NOT NULL
FK plan_id -> plans(id) ON DELETE CASCADE
FK invited_by_user_id -> users(id) ON DELETE CASCADE
```

### `plan_collaborators`

```text
id                  TEXT PK
plan_id             TEXT NOT NULL
user_id             TEXT NOT NULL
role                TEXT NOT NULL -- owner | editor | viewer
invited_by_user_id  TEXT NOT NULL
created_at          TIMESTAMP NOT NULL
updated_at          TIMESTAMP NOT NULL
FK plan_id -> plans(id) ON DELETE CASCADE
FK user_id -> users(id) ON DELETE CASCADE
FK invited_by_user_id -> users(id) ON DELETE CASCADE
UNIQUE(plan_id, user_id)
```

## Backend implementation plan

### Existing repository context

The repository already includes a collaboration-related migration surface, so this feature should reuse and extend those patterns rather than creating a parallel access model.

### New backend work

- Extend the existing collaborators domain to support plan-scoped roles
- Add an invitation flow with accept/reject/revoke states
- Enforce permission checks in plan/scenario write operations
- Add a lightweight activity log for invite creation, role updates, and access changes

### API endpoints

- `GET /api/plans/{id}/collaborators` — list collaborators for a plan
- `POST /api/plans/{id}/collaborators/invite` — invite a user by email
- `PUT /api/plans/{id}/collaborators/{userId}` — change a collaborator role
- `DELETE /api/plans/{id}/collaborators/{userId}` — remove access
- `POST /api/collaboration-invites/{id}/accept` — accept an invitation
- `POST /api/collaboration-invites/{id}/decline` — decline an invitation

## Frontend implementation plan

### UI additions

- Add a “Share plan” action on plan detail screens
- Show a collaborator list with current role and access status
- Add an invitation dialog for email-based sharing
- Add a “Shared with me” section to the dashboard or plans list

### Files

- `frontend/src/api/types.ts` — collaborator and invite DTOs
- `frontend/src/api/client.ts` — collaboration API methods
- `frontend/src/pages/PlanDetailsPage.tsx` — share and role-management UI
- `frontend/src/pages/DashboardPage.tsx` — shared-plan listing

## Testing plan

- Backend tests for invitation lifecycle and role enforcement
- Tests proving that viewers cannot modify plan data
- Frontend tests for share dialog UI and collaborator role display

## Acceptance criteria

- A plan owner can invite another user to collaborate
- Collaborators can be assigned an editor or viewer role
- Viewer-role users cannot write to the plan
- Invitations can be accepted or declined
- Shared plans are visible to the invited user after acceptance

## Specialists needed

- `rust-developer`: permissions, invitations, and API integration
- `frontend-developer`: share UI and shared-plan experience
- `test-writer`: access-control and invitation lifecycle coverage
