# Generated from api/operations.json. Do not edit by hand.
from __future__ import annotations

import json
import subprocess
import uuid
from pathlib import Path
from typing import Any, Iterable

SCHEMA_VERSION = "structural-automation/1.0"
API_VERSION = "1.1"


class AutomationError(RuntimeError):
    def __init__(self, code: str, message: str, response: dict[str, Any]):
        super().__init__(f"{code}: {message}")
        self.code = code
        self.response = response


class StructuralAutomationClient:
    """Dependency-free client for the local structural-automation executable."""

    def __init__(self, executable: str = "structural-automation"):
        self.executable = executable

    def call(
        self,
        method: str,
        params: dict[str, Any] | None = None,
        capabilities: Iterable[str] = (),
    ) -> Any:
        request = {
            "schema_version": SCHEMA_VERSION,
            "request_id": str(uuid.uuid4()),
            "method": method,
            "params": params or {},
            "capabilities": list(capabilities),
        }
        completed = subprocess.run(
            [self.executable, "serve"],
            input=json.dumps(request, separators=(",", ":")) + "\n",
            text=True,
            capture_output=True,
            check=False,
        )
        if completed.returncode != 0:
            raise AutomationError(
                "host_failed", completed.stderr.strip() or "automation host failed", {}
            )
        lines = [line for line in completed.stdout.splitlines() if line.strip()]
        if len(lines) != 1:
            raise AutomationError("protocol_error", "expected one JSON response", {})
        response = json.loads(lines[0])
        if response.get("status") != "ok":
            error = response.get("error") or {}
            raise AutomationError(
                error.get("code", "unknown_error"),
                error.get("message", "automation request failed"),
                response,
            )
        return response.get("result")

    def describe(self) -> dict[str, Any]:
        return self.call("system.describe", capabilities=["inspect_system"])

    def sha256_text(self, text: str) -> dict[str, Any]:
        return self.call(
            "artifact.sha256", {"text": text}, capabilities=["read_artifacts"]
        )

    def validate_model(self, analysis_input: dict[str, Any]) -> dict[str, Any]:
        return self.call(
            "model.validate",
            {"input": analysis_input},
            capabilities=["validate_models"],
        )

    def evaluate_rules(
        self, package: dict[str, Any], evaluation_input: dict[str, Any]
    ) -> dict[str, Any]:
        return self.call(
            "rules.evaluate",
            {"package": package, "input": evaluation_input},
            capabilities=["evaluate_rules"],
        )

    def solve_nonlinear(
        self,
        analysis_input: dict[str, Any],
        options: dict[str, Any],
        restart: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {"input": analysis_input, "options": options}
        if restart is not None:
            params["restart"] = restart
        return self.call(
            "analysis.solve_nonlinear",
            params,
            capabilities=["run_analysis"],
        )
