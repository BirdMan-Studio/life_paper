"""Python API supplied to each organism's main(game) entry point."""
from collections.abc import Mapping
from types import MappingProxyType
import math


class ReadOnly(Mapping):
    def __init__(self, values):
        object.__setattr__(self, "_values", MappingProxyType({key: freeze(value) for key, value in values.items()}))

    def __getitem__(self, key):
        return self._values[key]

    def __iter__(self):
        return iter(self._values)

    def __len__(self):
        return len(self._values)

    def __getattr__(self, key):
        try:
            return self._values[key]
        except KeyError:
            raise AttributeError(key) from None

    def __setattr__(self, key, value):
        raise AttributeError("game state is read-only; use game.memory for AI state")


def freeze(value):
    if isinstance(value, dict):
        return ReadOnly(value)
    if isinstance(value, list):
        return tuple(freeze(item) for item in value)
    return value


class Component(ReadOnly):
    def __init__(self, values, query):
        super().__init__(values)
        object.__setattr__(self, "_query", query)

    def _control(self, method, rate, **params):
        if type(rate) not in (int, float) or not math.isfinite(rate):
            raise TypeError("rate must be a finite number in mg/tick")
        command = self._query("component_command", {
            "component_id": self.id, "method": method,
            "rate_mg_per_tick": rate, **params,
        })
        return command["action"]["rate_mg_per_tick"]


class JetControl:
    def jet(self, material, rate):
        """Record the requested rate unchanged; the latest component output wins."""
        if not isinstance(material, str):
            raise TypeError("material must be a registered substance ID")
        return self._control("jet", rate, material=material)


class AbsorptionControl:
    def absorb_air(self, rate):
        """Request air absorption in mg/tick; world execution checks submersion."""
        return self._control("absorb_air", rate)

    def absorb_matter(self, rate):
        """Request nearby matter absorption in mg/tick."""
        return self._control("absorb_matter", rate)


class VisionControl:
    def get_vision(self, angles=None):
        """Reserved for world lighting/occlusion; currently raises NotImplementedError."""
        if angles is not None:
            angles = list(angles)
            if not 1 <= len(angles) <= 120 or any(
                type(angle) not in (int, float) or not math.isfinite(angle) or not -30 <= angle <= 30
                for angle in angles
            ):
                raise ValueError("angles must contain 1..120 finite angles within -30..30 degrees")
        return self._query("component_vision", {"component_id": self.id, "angles": angles})


def make_component(values, query, classes):
    api = values["api"]
    mixins = tuple(cls for key, cls in (
        ("jet", JetControl), ("absorption", AbsorptionControl), ("vision", VisionControl)
    ) if api.get(key))
    if mixins not in classes:
        classes[mixins] = type("OrganismComponent", (*mixins, Component), {})
    return classes[mixins](values, query)


class World(ReadOnly):
    def __init__(self, values, query):
        values = dict(values)
        initial_tiles = values.pop("_tiles", [])
        super().__init__(values)
        object.__setattr__(self, "_query", query)
        object.__setattr__(self, "_tiles", {(tile["x"], tile["y"]): freeze(tile) for tile in initial_tiles})

    def get_tile(self, x, y):
        """Read one ground pixel. Inaccessible pixels raise PermissionError."""
        if type(x) is not int or type(y) is not int:
            raise TypeError("tile coordinates must be integers")
        if (x, y) not in self._tiles:
            self._tiles[x, y] = freeze(self._query("get_tile", {"x": x, "y": y}))
        return self._tiles[x, y]

    def get_organisms(self, offset=0, limit=100):
        """Read a page of visible organism summaries, excluding their private AI state."""
        if type(offset) is not int or type(limit) is not int or offset < 0 or not 1 <= limit <= 100:
            raise ValueError("offset must be >= 0 and limit must be between 1 and 100")
        return freeze(self._query("get_organisms", {"offset": offset, "limit": limit}))

    def get_tiles(self, points):
        """Read up to 256 (x, y) pairs in one request; permissions apply to every pixel."""
        points = list(points)
        if not 1 <= len(points) <= 256:
            raise ValueError("get_tiles requires between 1 and 256 points")
        normalized = []
        missing = {}
        for point in points:
            if not isinstance(point, (tuple, list)) or len(point) != 2 or any(type(value) is not int for value in point):
                raise TypeError("each point must be an integer (x, y) pair")
            key = tuple(point)
            normalized.append(key)
            if key not in self._tiles:
                missing[key] = {"x": point[0], "y": point[1]}
        if missing:
            for tile in self._query("get_tiles", {"points": list(missing.values())}):
                self._tiles[tile["x"], tile["y"]] = freeze(tile)
        return tuple(self._tiles[key] for key in normalized)


class Game(ReadOnly):
    def __init__(self, context, query, log):
        me = dict(context["me"])
        classes = {}
        me["components"] = {key: make_component(value, query, classes) for key, value in me["components"].items()}
        super().__init__({
            "me": me,
            "time": context["time"],
            "tutorial": context["tutorial"],
        })
        object.__setattr__(self, "world", World(context["world"], query))
        object.__setattr__(self, "memory", context["memory"])
        object.__setattr__(self, "_log", log)

    def log(self, *values):
        self._log(" ".join(str(value) for value in values) + "\n")
