import os
import sys
import pytest
from unittest.mock import MagicMock

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from pipeql_python.driver import create_pipeql_driver

def test_driver_override_dialect():
    mock_conn = MagicMock()
    # Test setting both driver and dialect
    driver = create_pipeql_driver(mock_conn, dialect="sqlite", driver="dbapi")

    assert driver.driver == "dbapi"
    assert driver.dialect == "sqlite"

def test_driver_override_dialect_inference():
    mock_conn = MagicMock()
    # Test inferring dialect from an explicitly set driver
    driver = create_pipeql_driver(mock_conn, driver="psycopg")

    assert driver.driver == "psycopg"
    assert driver.dialect == "postgres"

def test_driver_dialect_unsupported_without_dialect():
    mock_conn = MagicMock()
    # Test failure when setting an unknown driver and dialect is not provided
    with pytest.raises(ValueError, match="cannot infer a target dialect for driver 'unknown'"):
        create_pipeql_driver(mock_conn, driver="unknown")

def test_driver_auto_detect_unsupported_driver():
    class UnknownConn:
        pass
    mock_conn = UnknownConn()
    with pytest.raises(ValueError, match="unsupported database connection"):
        create_pipeql_driver(mock_conn)
