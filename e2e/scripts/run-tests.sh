#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Generate a unique run id + directory so each run's artifacts are self-contained.
RUN_ID="$(date +%Y%m%d_%H%M%S)_$(openssl rand -hex 4 2>/dev/null || echo $RANDOM)"
BASE_DIR="${LIP_E2E_BASE_DIR:-/tmp/lip-e2e}"
export RUN_DIR="${BASE_DIR}/${RUN_ID}"

echo "Run ID  : ${RUN_ID}"
echo "Run dir : ${RUN_DIR}"
echo ""

mkdir -p \
  "${RUN_DIR}/service-logs" \
  "${RUN_DIR}/browser-logs" \
  "${RUN_DIR}/playwright-artifacts" \
  "${RUN_DIR}/playwright-report"

# Separate --report from any playwright-specific args (e.g. a test path).
PW_ARGS=()
SHOW_REPORT=false
for arg in "$@"; do
  case "$arg" in
    --report) SHOW_REPORT=true ;;
    *) PW_ARGS+=("$arg") ;;
  esac
done

cd "$E2E_ROOT"
EXIT_CODE=0
# global-setup starts the test backend + frontend; global-teardown stops them.
npx playwright test "${PW_ARGS[@]+"${PW_ARGS[@]}"}" || EXIT_CODE=$?

echo ""
echo "Run artifacts saved to: ${RUN_DIR}"
echo "  Service logs : ${RUN_DIR}/service-logs/"
echo "  Browser logs : ${RUN_DIR}/browser-logs/"
echo "  PW artifacts : ${RUN_DIR}/playwright-artifacts/"
echo "  PW report    : ${RUN_DIR}/playwright-report/"

if [ "$SHOW_REPORT" = true ]; then
  npx playwright show-report "${RUN_DIR}/playwright-report"
fi

exit $EXIT_CODE
