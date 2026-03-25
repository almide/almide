from .almide import AlmideAdapter
from .python import PythonAdapter
from .typescript import TypeScriptAdapter

ADAPTERS = {
    "almide": AlmideAdapter,
    "python": PythonAdapter,
    "ts": TypeScriptAdapter,
}
