"""Simulation configuration, read from a JSON file and validated."""

import json
from dataclasses import dataclass

# field name -> expected type
FIELDS = {
    "floors": int,
    "elevators": int,
    "capacity": int,
    "speed": float,
    "door_time": float,
    "seed": int,
    "rate": float,
    "duration": float,
}


@dataclass(frozen=True)
class Config:
    floors: int
    elevators: int
    capacity: int
    speed: float  # seconds per floor
    door_time: float  # seconds with the door open
    seed: int
    rate: float  # passengers per minute, at each origin floor
    duration: float  # seconds during which passengers may arrive

    def __post_init__(self):
        if self.floors < 2:
            raise ValueError("floors must be >= 2")
        if self.elevators < 1:
            raise ValueError("elevators must be >= 1")
        if self.capacity < 1:
            raise ValueError("capacity must be >= 1")
        if self.speed <= 0:
            raise ValueError("speed must be > 0")
        if self.door_time < 0:
            raise ValueError("door_time must be >= 0")
        if self.rate < 0:
            raise ValueError("rate must be >= 0")
        if self.duration < 0:
            raise ValueError("duration must be >= 0")


def from_dict(data):
    if not isinstance(data, dict):
        raise ValueError("the configuration must be a JSON object")
    missing = sorted(set(FIELDS) - set(data))
    if missing:
        raise ValueError("missing fields: " + ", ".join(missing))
    unknown = sorted(set(data) - set(FIELDS))
    if unknown:
        raise ValueError("unknown fields: " + ", ".join(unknown))
    return Config(**{name: kind(data[name]) for name, kind in FIELDS.items()})


def load(path):
    with open(path, encoding="utf-8") as file:
        return from_dict(json.load(file))
