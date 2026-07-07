/** @type {import('jest').Config} */
module.exports = {
  testEnvironment: "jsdom",
  setupFilesAfterEnv: ["<rootDir>/jest.setup.ts"],
  transform: {
    "^.+\\.tsx?$": [
      "ts-jest",
      { tsconfig: { jsx: "react-jsx", esModuleInterop: true } },
    ],
  },
  moduleNameMapper: {
    "\\.(css)$": "identity-obj-proxy",
  },
  testMatch: ["<rootDir>/src/**/*.test.ts?(x)"],
};
