"""Validate the operation catalog used to generate the checked-in Python client.

The client template is checked in for review. This command verifies the catalog,
the generic call method, and the convenience helpers maintained by this SDK.
"""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
CATALOG_PATH = ROOT / "api" / "operations.json"
CLIENT_PATH = ROOT / "bindings" / "python" / "structural_api" / "client.py"

REQUIRED_HELPERS = {
    "system.describe": "def describe(",
    "artifact.sha256": "def sha256_text(",
    "model.validate": "def validate_model(",
    "rules.evaluate": "def evaluate_rules(",
    "analysis.solve_nonlinear": "def solve_nonlinear(",
}


def load_catalog() -> dict[str, Any]:
    try:
        catalog = json.loads(CATALOG_PATH.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise SystemExit(f"operation catalog not found: {CATALOG_PATH}") from exc
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid JSON in {CATALOG_PATH}: {exc}") from exc

    if not isinstance(catalog, dict):
        raise SystemExit("operation catalog root must be a JSON object")

    operations = catalog.get("operations")
    if not isinstance(operations, list):
        raise SystemExit("operation catalog must contain an 'operations' array")

    return catalog


def operation_methods(catalog: dict[str, Any]) -> list[str]:
    """Return method names while allowing non-operation metadata records."""
    methods: list[str] = []

    for index, operation in enumerate(catalog["operations"]):
        if not isinstance(operation, dict):
            raise SystemExit(
                f"operations[{index}] must be an object, got "
                f"{type(operation).__name__}"
            )

        # Some newer catalogs contain group/header/metadata records in the
        # operations array. They intentionally have no RPC method.
        method = operation.get("method")
        if method is None:
            continue
        if not isinstance(method, str) or not method.strip():
            raise SystemExit(
                f"operations[{index}].method must be a non-empty string"
            )

        methods.append(method)

    return methods


def main() -> None:
    catalog = load_catalog()
    methods = operation_methods(catalog)

    try:
        client = CLIENT_PATH.read_text(encoding="utf-8")
    except FileNotFoundError as exc:
        raise SystemExit(f"generated client not found: {CLIENT_PATH}") from exc

    if "def call(" not in client:
        raise SystemExit("generated client is missing the generic call() method")

    missing_helpers = [
        method
        for method, helper_signature in REQUIRED_HELPERS.items()
        if method in methods and helper_signature not in client
    ]
    if missing_helpers:
        raise SystemExit(
            "generated client is missing helpers for: "
            + ", ".join(missing_helpers)
        )

    api_version = catalog.get("api_version", "unknown")
    helper_count = sum(method in methods for method in REQUIRED_HELPERS)
    print(
        f"Python binding matches API {api_version} "
        f"({len(methods)} catalog operations, {helper_count} convenience helpers)"
    )


if __name__ == "__main__":
    main()
