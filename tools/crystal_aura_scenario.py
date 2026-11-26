#!/usr/bin/env python3
"""CrystalAura host scenario in a fresh isolated world.

Builds a wall-sheltered obsidian block beside a NoAI zombie, gives the
player end crystals and netherite armor, and enables the module. The auto
loop must aim at the interaction point (silent rotation), place a crystal
after the activation delay and attack it after the action delay; the
explosion then damages the zombie. The checks read the host's entities and
health through the control socket.
"""
import argparse
import json
import os

from native_session import NativeSession
from smoke_scenario import Checks

BIND = 'KeyV'
FLOOR = 99
BASE = (3, 100, 0)
MOB = ('cow', (4.5, 100.0, 0.5))


def command(session, text):
    session.call('chat', '/' + text)
    # The command runs on the next tick; the control socket only types it.
    session.call('wait', '4')


def mob_health(session):
    reply = session.call('mobs', '16')
    for mob in reply.get('near', []):
        if mob.get('kind', '').endswith(MOB[0]):
            return mob.get('health')
    return None


def enabled(session):
    return session.saved()['modules'].get('combat.crystal-aura', {}).get('enabled', False)


def run(resources, allow_stale):
    with NativeSession(resources, 'crystal-aura-', allow_stale=allow_stale) as session:
        checks = Checks(session.root)
        session.seed({
            'schema': 1,
            'modules': {},
            'settings': {
                'combat.crystal-aura.binds': [BIND],
                'combat.crystal-aura.range': 4.5,
                'combat.crystal-aura.max-self-damage': 20.0,
                'combat.crystal-aura.anti-suicide': True,
                'combat.crystal-aura.aim-speed': 10.0,
                'combat.crystal-aura.delay': [0.0, 0.0],
                'combat.crystal-aura.action-delay': [0.0, 0.0],
            },
        })
        session.launch(creative=True)
        session.call('time', '6000')
        session.call('tp', '0.5', '100', '0.5')
        session.call('wait-ready', '40')
        for _ in range(10):
            session.call('fill', '-4', str(FLOOR), '-4', '8', str(FLOOR), '8', 'minecraft:stone')
            session.call('fill', '-4', '100', '-4', '8', '104', '8', 'minecraft:air')
            counts = session.call('blocks', '-4', str(FLOOR), '-4', '8', str(FLOOR), '8').get(
                'counts', {})
            if counts.get('minecraft:stone') == 13 * 13:
                break
            session.call('wait', '20')
        checks.check('scenario floor is in place', counts.get('minecraft:stone') == 13 * 13, counts)
        # The first teleport happens before the floor exists, so the player
        # falls; stand on the finished floor before placing the base.
        session.call('tp', '0.5', '100', '0.5')
        session.call('wait', '4')
        # A one-block wall shelters the player's body while the eye still
        # sees the obsidian's top face.
        command(session, 'setblock 2 100 0 minecraft:stone')
        command(session, 'setblock 3 100 0 minecraft:obsidian')
        checks.check(
            'the obsidian base is in place',
            session.call('block', '3', '100', '0').get('state', '').endswith('obsidian'),
            session.call('block', '3', '100', '0'),
        )
        for slot, item in (
            ('head', 'netherite_helmet'),
            ('chest', 'netherite_chestplate'),
            ('legs', 'netherite_leggings'),
            ('feet', 'netherite_boots'),
        ):
            command(session, f'item replace entity @s armor.{slot} with minecraft:{item}')
        command(session, 'item replace entity @s hotbar.0 with minecraft:end_crystal 16')
        kind, at = MOB
        session.call('summon', kind, str(at[0]), str(at[1]), str(at[2]))
        before = None
        for _ in range(10):
            session.call('wait', '10')
            before = mob_health(session)
            if before is not None:
                break
        checks.check('a target mob stands by the base', before is not None, before)
        session.call('tp', '0.5', '100', '0.5')
        session.call('look', '0', '0')
        # Crouching shrinks the box the explosion samples, so the wall
        # shelters most of it.
        session.call('key', 'shift', 'down')
        session.call('wait', '4')
        checks.check('CrystalAura starts disabled', not enabled(session))
        session.call('key', BIND[3:].lower(), 'tap')
        session.call('wait', '2')
        checks.check('CrystalAura enabled through its bind', enabled(session))

        after = before
        if before is not None:
            for _ in range(30):
                session.call('wait', '10')
                after = mob_health(session)
                if after is None or after < before:
                    break
        checks.check(
            'the placed crystal explodes on the target',
            before is not None and (after is None or after < before),
            {'before': before, 'after': after},
        )
        state = session.call('state')
        health = state.get('player', {}).get('health')
        checks.check('the player survives the blast', health is not None and health > 0.0,
                     state.get('player'))
        session.call('key', 'shift', 'up')
        session.capture('final')
        ok = all(result['ok'] for result in checks.results)
        print('PASS' if ok else 'FAIL',
              f'{sum(result["ok"] for result in checks.results)}/{len(checks.results)}')
        return ok


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('resources', nargs='?',
                        default=os.environ.get('HORIZON_RESOURCES'),
                        help='prepared Minecraft 26.3 resources (or set HORIZON_RESOURCES)')
    parser.add_argument('--allow-stale', action='store_true')
    args = parser.parse_args()
    if not args.resources:
        parser.error('pass the resources directory or set HORIZON_RESOURCES')
    raise SystemExit(0 if run(args.resources, args.allow_stale) else 1)


if __name__ == '__main__':
    main()
