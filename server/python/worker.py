import builtins
import contextlib
import importlib.abc
import importlib.util
import inspect
import io
import json
import os
import shutil
import sys
import tempfile
import threading
import time
import traceback
import types
from collections import OrderedDict
from pathlib import Path


protocol_out = sys.stdout
protocol_in = sys.stdin


def send(value):
    protocol_out.write(json.dumps(value, ensure_ascii=True, allow_nan=False) + "\n")
    protocol_out.flush()


class ProgramLoader(importlib.abc.Loader):
    def __init__(self, root, relative, cache):
        self.root, self.relative, self.cache = root, relative, cache

    def create_module(self, spec):
        return None

    def exec_module(self, module):
        if self.relative not in self.cache:
            path = self.root / self.relative
            self.cache[self.relative] = compile(path.read_bytes(), str(path), "exec")
        exec(self.cache[self.relative], module.__dict__)


class ProgramFinder(importlib.abc.MetaPathFinder):
    def __init__(self, root, cache):
        self.root, self.cache = root, cache

    def find_spec(self, fullname, path=None, target=None):
        relative = fullname.replace(".", "/")
        package = self.root / relative
        for filename, is_package in [(relative + "/__init__.py", True), (relative + ".py", False)]:
            source = self.root / filename
            if source.is_file():
                return importlib.util.spec_from_file_location(
                    fullname, source, loader=ProgramLoader(self.root, filename, self.cache),
                    submodule_search_locations=[str(package)] if is_package else None,
                )
        if package.is_dir():
            spec = importlib.util.spec_from_loader(fullname, loader=None, is_package=True)
            spec.submodule_search_locations = [str(package)]
            return spec
        return None


def invoke(payload, sdk_code, code_cache, cache_limit, workspace):
    root = Path(payload["program"]).resolve()
    key = (str(root), payload["version"])
    cache = code_cache.pop(key, {})
    cache_hit = "main.py" in cache
    code_cache[key] = cache
    while len(code_cache) > cache_limit:
        code_cache.popitem(last=False)
    context = payload["context"]
    sdk = types.ModuleType("life_game")
    exec(sdk_code, sdk.__dict__)
    logs = io.StringIO()
    log_size = 0

    def log(text):
        nonlocal log_size
        remaining = max(0, 16384 - log_size)
        logs.write(text[:remaining])
        log_size += min(len(text), remaining)

    class LogStream(io.TextIOBase):
        def write(self, text):
            log(text)
            return len(text)

    def query(method, params):
        send({"type": "query", "call_id": payload["call_id"], "method": method, "params": params})
        response = json.loads(protocol_in.readline())
        if "error" in response:
            error = response["error"]
            if error["code"] == "FORBIDDEN":
                raise PermissionError(error["message"])
            if error["code"] == "DISABLED":
                raise RuntimeError(error["message"])
            if error["code"] == "NOT_IMPLEMENTED":
                raise NotImplementedError(error["message"])
            raise ValueError(error["message"])
        return response["data"]

    # Cache code objects, never live user modules or their globals.
    baseline_modules = sys.modules.copy()
    baseline_builtins = builtins.__dict__.copy()
    baseline_path, baseline_finders = sys.path[:], sys.meta_path[:]
    baseline_env, baseline_cwd = dict(os.environ), os.getcwd()
    baseline_threads = set(threading.enumerate())
    scratch = tempfile.mkdtemp(prefix="call-", dir=workspace)
    result = {"type": "result", "ok": False, "memory": None, "logs": "", "error": None,
              "code_cache_hit": cache_hit, "load_us": 0, "execution_us": 0, "reusable": True}
    game = sdk.Game(context, query, log)
    try:
        os.chdir(scratch)
        sys.path.insert(0, str(root))
        sys.meta_path.insert(0, ProgramFinder(root, cache))
        sys.modules["life_game"] = sdk
        sys.dont_write_bytecode = True
        with contextlib.redirect_stdout(LogStream()), contextlib.redirect_stderr(LogStream()):
            started = time.perf_counter_ns()
            spec = importlib.util.spec_from_file_location("main", root / "main.py", loader=ProgramLoader(root, "main.py", cache))
            module = importlib.util.module_from_spec(spec)
            sys.modules["main"] = module
            try:
                spec.loader.exec_module(module)
            finally:
                result["load_us"] = (time.perf_counter_ns() - started) // 1000
            entry = getattr(module, "main", None)
            if not callable(entry):
                raise TypeError("main.py must define def main(game)")
            if inspect.iscoroutinefunction(entry):
                raise TypeError("main(game) must be a synchronous function")
            started = time.perf_counter_ns()
            try:
                value = entry(game)
                if inspect.isawaitable(value):
                    raise TypeError("main(game) must not return an awaitable")
            finally:
                result["execution_us"] = (time.perf_counter_ns() - started) // 1000
        if not isinstance(game.memory, dict):
            raise TypeError("game.memory must be a dictionary")
        memory_json = json.dumps(game.memory, ensure_ascii=True, allow_nan=False)
        if len(memory_json.encode("utf-8")) > 16384:
            raise ValueError("game.memory exceeds 16 KiB")
        result.update(ok=True, memory=game.memory)
    except BaseException:
        result["error"] = traceback.format_exc(limit=8)[-8192:]
    finally:
        result["logs"] = logs.getvalue()
        sys.path[:] = baseline_path
        sys.meta_path[:] = baseline_finders
        for name in list(sys.modules):
            if name not in baseline_modules:
                del sys.modules[name]
        sys.modules.update(baseline_modules)
        builtins.__dict__.clear()
        builtins.__dict__.update(baseline_builtins)
        os.environ.clear()
        os.environ.update(baseline_env)
        os.chdir(baseline_cwd)
        if set(threading.enumerate()) != baseline_threads:
            result["reusable"] = False
        shutil.rmtree(scratch)
    return result


def run():
    init = json.loads(protocol_in.readline())
    sdk_code = compile(init["sdk"], "life_game.py", "exec")
    code_cache = OrderedDict()
    send({"type": "ready", "pid": os.getpid()})
    for line in protocol_in:
        payload = json.loads(line)
        result = invoke(payload, sdk_code, code_cache, init["cache_entries"], init["workspace"])
        result["call_id"] = payload["call_id"]
        send(result)
        if not result["reusable"]:
            break


run()
