import { api, ApiError, clearToken, getToken, setToken } from "./client";

describe("token storage", () => {
  afterEach(() => clearToken());

  it("persists and clears the token", () => {
    expect(getToken()).toBeNull();
    setToken("abc123");
    expect(getToken()).toBe("abc123");
    clearToken();
    expect(getToken()).toBeNull();
  });
});

describe("api client", () => {
  const fetchMock = jest.fn();

  beforeEach(() => {
    fetchMock.mockReset();
    global.fetch = fetchMock as unknown as typeof fetch;
    clearToken();
  });

  it("attaches the bearer token when present", async () => {
    setToken("tok");
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ id: "1", email: "a@b.com", created_at: "" }),
    });

    await api.me();

    const [, init] = fetchMock.mock.calls[0];
    expect(init.headers.Authorization).toBe("Bearer tok");
  });

  it("throws ApiError with the server message on failure", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 401,
      json: async () => ({ error: "Invalid email or password" }),
    });

    await expect(api.login("a@b.com", "nope")).rejects.toMatchObject({
      status: 401,
      message: "Invalid email or password",
    } satisfies Partial<ApiError>);
  });
});
