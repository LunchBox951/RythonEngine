"""
npc/skeleton.py — Skeleton NPC AI (functional state machine).

States:
  PATROL  — roam waypoints at 3 u/s (boss: 4 u/s)
  CHASE   — pursue player at 5 u/s (boss: 7 u/s), trigger at 10 u (boss: 15 u)
  ATTACK  — emit "enemy_attack" every 1.5 s at range 1.5 u (boss: 20 dmg, 2 u)

Interface (consumed by enemies.py):
    create_state(entity, is_boss) -> dict
    update(state, dt, px, py, pz) -> None
"""
from __future__ import annotations

import math
import rython

PATROL = "PATROL"
CHASE = "CHASE"
ATTACK = "ATTACK"


def create_state(entity, is_boss: bool = False) -> dict:
    """Build initial AI state dict for a skeleton entity.

    Patrol waypoints are generated as a small square around the spawn position.
    """
    t = entity.transform
    sx, sy, sz = t.x, t.y, t.z
    r = 3.0
    waypoints = [
        (sx + r, sy, sz),
        (sx, sy, sz + r),
        (sx - r, sy, sz),
        (sx, sy, sz - r),
    ]
    return {
        "entity": entity,
        "is_boss": is_boss,
        "state": PATROL,
        "waypoints": waypoints,
        "wp_index": 0,
        "attack_timer": 0.0,
        "alive": True,
        "patrol_speed": 4.0 if is_boss else 3.0,
        "chase_speed": 7.0 if is_boss else 5.0,
        "chase_range": 15.0 if is_boss else 10.0,
        "attack_range": 2.0 if is_boss else 1.5,
        "attack_damage": 20 if is_boss else 10,
        "attack_cooldown": 1.5,
    }


def update(state: dict, dt: float, px: float, py: float, pz: float) -> None:
    """Tick one skeleton AI state. Called by enemies.update() each frame."""
    if not state["alive"]:
        return

    entity = state["entity"]
    try:
        tf = entity.transform
    except Exception:
        state["alive"] = False
        return

    dx = px - tf.x
    dz = pz - tf.z
    dist = math.sqrt(dx * dx + dz * dz)

    chase_range = state["chase_range"]
    attack_range = state["attack_range"]
    current = state["state"]

    # State transitions
    if current == PATROL:
        if dist <= chase_range:
            state["state"] = CHASE
    elif current == CHASE:
        if dist > chase_range * 1.2:
            state["state"] = PATROL
        elif dist <= attack_range:
            state["state"] = ATTACK
            try:
                entity.set_velocity(0.0, entity.velocity.y, 0.0)
            except Exception:
                pass
    elif current == ATTACK:
        if dist > attack_range * 1.5:
            state["state"] = CHASE

    # Behaviour
    if state["state"] == PATROL:
        _do_patrol(state, entity, tf, dt)
    elif state["state"] == CHASE:
        _do_chase(state, entity, tf, px, pz)
    elif state["state"] == ATTACK:
        _do_attack(state, dt)


def _do_patrol(state: dict, entity, tf, dt: float) -> None:
    wps = state["waypoints"]
    if not wps:
        return
    tx, _, tz = wps[state["wp_index"]]
    dx = tx - tf.x
    dz = tz - tf.z
    dist = math.sqrt(dx * dx + dz * dz)
    if dist < 0.5:
        state["wp_index"] = (state["wp_index"] + 1) % len(wps)
    else:
        speed = state["patrol_speed"]
        nx, nz = dx / dist, dz / dist
        try:
            entity.set_velocity(nx * speed, entity.velocity.y, nz * speed)
        except Exception:
            pass


def _do_chase(state: dict, entity, tf, px: float, pz: float) -> None:
    dx = px - tf.x
    dz = pz - tf.z
    dist = math.sqrt(dx * dx + dz * dz)
    if dist < 0.1:
        return
    speed = state["chase_speed"]
    nx, nz = dx / dist, dz / dist
    try:
        entity.set_velocity(nx * speed, entity.velocity.y, nz * speed)
    except Exception:
        pass


def _do_attack(state: dict, dt: float) -> None:
    state["attack_timer"] += dt
    if state["attack_timer"] >= state["attack_cooldown"]:
        state["attack_timer"] = 0.0
        rython.scene.emit("enemy_attack", damage=state["attack_damage"])
        rython.audio.play("game/assets/sounds/sfx/chop.ogg", "sfx", False)
