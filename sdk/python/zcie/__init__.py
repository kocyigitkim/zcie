"""ZCIE Python SDK: a client of the local ZCIE daemon HTTP API (docs/10-api-and-gateways.md).

Standard library only. Python 3.9 or later.
"""

from .client import DEFAULT_ENDPOINT, EVENT_SCHEMA, ID_FILE, Client, ZcieError, find_project_id

__all__ = ["DEFAULT_ENDPOINT", "EVENT_SCHEMA", "ID_FILE", "Client", "ZcieError", "find_project_id"]
__version__ = "0.1.0"
