#!/usr/bin/env python3
"""AutoAnchor host scenario in a fresh isolated world.

Gives the player a respawn anchor and glowstone, aims at a floor block and
enables the On bind mode. The macro must place the anchor, charge it with
glowstone, swap back to the anchor and detonate it; the module then disables
and the anchor block is gone. The checks read the host blocks and module
state through the control socket.
"""
import argparse
import os

from native_session import NativeSession
from smoke_scenario import Checks

BIND = 'KeyB'
BASE = (2, 99, 0)
ANCHOR = (2, 100, 0)
LOOK = (-90.0, 39.0)


def command(session, text):
    session.call('chat', '/' + text)
    session.call('wait', '4')


def enabled(session):
    return session.saved()['modules'].get('utility.auto-anchor', {}).get('enabled', False)


def block(session, pos):
    return session.call('block', str(pos[0]), str(pos[1]), str(pos[2])).get('state', '')


def run(resources, allow_stale):
    with NativeSession(resources, 'auto-anchor-', allow_stale=allow_stale) as session:
        checks = Checks(session.root)
        session.seed({
            'schema': 1,
            'modules': {},
            'settings': {
                'utility.auto-anchor.binds': [BIND],
                'utility.auto-anchor.mode': 'On bind',
                'utility.auto-anchor.double-anchor': False,
                'utility.auto-anchor.safe-anchor': False,
                'utility.auto-anchor.explosion-item-whitelist': False,
                'utility.auto-anchor.aim-assist': True,
                'utility.auto-anchor.silent-aim': True,
                'utility.auto-anchor.aim-speed': 12.0,
                # The reference default is 50-100 ms; a slower delay lets
                # the scenario observe the placed anchor before it detonates.
                'utility.auto-anchor.delay': [200.0, 200.0],
            },
        })
        session.launch(creative=True)
        session.call('time', '6000')
        session.call('tp', '0.5', '100', '0.5')
        session.call('wait-ready', '40')
        counts = {}
        for _ in range(10):
            session.call('fill', '-4', '99', '-4', '8', '99', '8', 'minecraft:stone')
            session.call('fill', '-4', '100', '-4', '8', '104', '8', 'minecraft:air')
            counts = session.call('blocks', '-4', '99', '-4', '8', '99', '8').get(
                'counts', {})
            if counts.get('minecraft:stone') == 13 * 13:
                break
            session.call('wait', '20')
        checks.check('scenario floor is in place',
                     counts.get('minecraft:stone') == 13 * 13, counts)
        session.call('tp', '0.5', '100', '0.5')
        session.call('wait', '4')
        command(session, 'item replace entity @s hotbar.0 with minecraft:respawn_anchor 16')
        command(session, 'item replace entity @s hotbar.1 with minecraft:glowstone 16')
        session.call('select', '1')
        session.call('look', str(LOOK[0]), str(LOOK[1]))
        session.call('wait', '4')
        looking = session.call('looking-at')
        pos = looking.get('block', {}).get('pos')
        checks.check('the crosshair is on the anchor base', pos == list(BASE), looking)
        checks.check('the anchor base is stone', block(session, BASE) == 'minecraft:stone',
                     block(session, BASE))
        checks.check('AutoAnchor starts disabled', not enabled(session))
        session.call('key', BIND[3:].lower(), 'tap')
        session.call('wait', '2')
        checks.check('AutoAnchor enabled through its bind', enabled(session))

        seen_anchor = False
        finished = False
        for _ in range(60):
            session.call('wait', '2')
            state = block(session, ANCHOR)
            if 'respawn_anchor' in state:
                seen_anchor = True
            if seen_anchor and 'respawn_anchor' not in state and not enabled(session):
                finished = True
                break
        checks.check('the anchor was placed', seen_anchor, block(session, ANCHOR))
        checks.check('the anchor detonated and the module finished', finished,
                     {'block': block(session, ANCHOR), 'enabled': enabled(session)})
        health = session.call('state').get('player', {}).get('health')
        checks.check('the player survives the blast', health is not None and health > 0.0,
                     health)
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