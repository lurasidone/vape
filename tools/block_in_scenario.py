#!/usr/bin/env python3
"""Block-In host scenario in a fresh isolated world.

Stands the player on a stone floor with stone in the hotbar, enables Block-In
through its bind, and checks the four walls at the feet level and the four
above are placed; the module then disables. The checks read the host blocks
through the control socket.
"""
import argparse
import os

from native_session import NativeSession
from smoke_scenario import Checks

BIND = 'KeyN'
FLOOR = 99
FEET = (0, 100, 0)
WALLS = [
    (1, 100, 0), (-1, 100, 0), (0, 100, 1), (0, 100, -1),
    (1, 101, 0), (-1, 101, 0), (0, 101, 1), (0, 101, -1),
]


def command(session, text):
    session.call('chat', '/' + text)
    session.call('wait', '4')


def enabled(session):
    return session.saved()['modules'].get('world.block-in', {}).get('enabled', False)


def block(session, pos):
    return session.call('block', str(pos[0]), str(pos[1]), str(pos[2])).get('state', '')


def run(resources, allow_stale):
    with NativeSession(resources, 'block-in-', allow_stale=allow_stale) as session:
        checks = Checks(session.root)
        session.seed({
            'schema': 1,
            'modules': {},
            'settings': {
                'world.block-in.binds': [BIND],
                'world.block-in.silent-aim': False,
                'world.block-in.sneak': False,
                'world.block-in.bed-finder': True,
                'world.block-in.block-priority': 'Lowest cost',
                'world.block-in.place-delay': [0.0, 0.0],
                'world.block-in.aim-speed': 12.0,
                'world.block-in.return-to-last-slot': True,
                'world.block-in.use-blacklist': True,
            },
        })
        session.launch(creative=True)
        session.call('time', '6000')
        session.call('tp', '0.5', '100', '0.5')
        session.call('wait-ready', '40')
        counts = {}
        for _ in range(10):
            session.call('fill', '-4', str(FLOOR), '-4', '8', str(FLOOR), '8',
                         'minecraft:stone')
            session.call('fill', '-4', '100', '-4', '8', '104', '8', 'minecraft:air')
            counts = session.call('blocks', '-4', str(FLOOR), '-4', '8', str(FLOOR), '8').get(
                'counts', {})
            if counts.get('minecraft:stone') == 13 * 13:
                break
            session.call('wait', '20')
        checks.check('scenario floor is in place',
                     counts.get('minecraft:stone') == 13 * 13, counts)
        session.call('tp', '0.5', '100', '0.5')
        session.call('wait', '4')
        command(session, 'item replace entity @s hotbar.0 with minecraft:stone 64')
        session.call('select', '1')
        session.call('wait', '4')
        checks.check('the player stands on the floor',
                     block(session, (0, FLOOR, 0)) == 'minecraft:stone',
                     block(session, (0, FLOOR, 0)))
        checks.check('Block-In starts disabled', not enabled(session))
        session.call('key', BIND[3:].lower(), 'tap')
        session.call('wait', '2')
        checks.check('Block-In enabled through its bind', enabled(session))

        placed = []
        for _ in range(120):
            session.call('wait', '2')
            placed = [pos for pos in WALLS if block(session, pos) == 'minecraft:stone']
            if len(placed) == len(WALLS) and not enabled(session):
                break
        checks.check('all eight enclosure walls are placed',
                     len(placed) == len(WALLS),
                     {'placed': len(placed), 'walls': WALLS})
        checks.check('Block-In disables when the enclosure is complete',
                     not enabled(session), enabled(session))
        checks.check('the player is still standing in the enclosure',
                     block(session, FEET) == 'minecraft:air',
                     block(session, FEET))
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