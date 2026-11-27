#!/usr/bin/env python3
"""The InventoryManager filter-rule editor, driven through the click GUI.

Opens the GUI, enables InventoryManager and expands its settings, scrolls the
frame to its Profiles setting, opens the editor popup and adds, cycles and
removes an item rule through clicks and physical keys. Every step checks the
saved `inventory.inventory-manager.profiles` JSON, so the popup's rendering and
its edits are the same path a player takes. The captures also show the GUI's
Geist text (the translations/fonts row: the reference checkout ships no font
or lang assets, so the labels are its English literals in the host fonts).

Coordinates are logical GUI pixels (the 800x600 window at GUI scale 2 is
400x300 logical). Frames are 110 wide at x=122 in the first column; the
editor popup is 300x220 centred.
"""
import argparse
import json
import subprocess
from pathlib import Path

import gui_layout
from native_session import NativeSession
from smoke_scenario import Checks

CATEGORY = gui_layout.category('Inventory')
MODULE = 'inventory.inventory-manager'
ROW = gui_layout.row('Inventory', MODULE)
DOTS = gui_layout.dots('Inventory', MODULE)
# The settings list starts at the module row top + 20 and iterates in id
# order: activation 18, binds 18, click-delay 24, then the three text rows
# presets, profile and profiles at 30 each (combat-check and open-inventory
# are hidden while the trigger is not "toggle"). The profiles EDIT button is
# at x + FRAME_WIDTH - 22, row top + 20. A 760-high window (380 logical)
# holds the whole expanded frame, so nothing needs scrolling.
PROFILES_ROW = ROW[1] + 10 + 18 + 18 + 24 + 30 + 30
EDIT = (122 + 110 - 22, PROFILES_ROW + 20)
# The popup panel is 300x220 centred on the 400x380 logical screen.
PANEL = (50.0, 80.0, 300.0, 220.0)
FIELD = (PANEL[0] + 8 + 112, PANEL[1] + PANEL[3] - 30 + 7)
ADD = (PANEL[0] + PANEL[2] - 50, PANEL[1] + PANEL[3] - 30 + 7)
TAB_Y = PANEL[1] + 10
PRESETS_TAB = (PANEL[0] + PANEL[2] - 140, TAB_Y)
ITEMS_TAB = (PANEL[0] + PANEL[2] - 52, TAB_Y)
NEW_PRESET = (PANEL[0] + PANEL[2] - 21, PANEL[1] + 26)
CLOSE = (PANEL[0] + PANEL[2] - 10, PANEL[1] + 9)
RULE = (PANEL[0] + 60, PANEL[1] + 38 + 7)
RULE_X = (PANEL[0] + PANEL[2] - 6 - 7, PANEL[1] + 38 + 7)


def run(resources):
    with NativeSession(resources, 'inv-editor-', size=(800, 760)) as session:
        checks = Checks(session.root)

        def click(point):
            session.call('click-at', str(point[0]), str(point[1]))

        def key(name):
            session.call('key', name, 'tap')

        def type_text(text):
            """Real X key events into the focused window: the socket's `key`
            only reaches the mod's input on the release, and the GUI's text
            fields take presses."""
            run = subprocess.run(['xdotool', 'type', '--delay', '40', text],
                                 env=session.env, text=True, capture_output=True)
            if run.returncode:
                raise RuntimeError(run.stderr)

        def capture(name):
            session.capture(name)

        def profiles():
            text = session.saved()['settings'].get(MODULE + '.profiles', '[]')
            return json.loads(text)

        def rules():
            found = profiles()
            return [r for p in found for r in p.get('items', [])]

        session.seed({'schema': 1, 'modules': {}, 'settings': {
            'client.gui.binds': ['ShiftLeft'],
        }})
        session.launch()
        session.call('wait', '2')
        session.refocus()
        key('shift')
        capture('gui')
        checks.check('the GUI opens', session.call('state')['screen'] == 'paused',
                     session.call('state')['screen'])

        click(CATEGORY)
        capture('inventory-frame')
        click(ROW)
        capture('module-enabled')
        checks.check('InventoryManager toggles from its row',
                     session.saved()['modules'][MODULE]['enabled'] is True)
        click(DOTS)
        session.call('wait', '2')
        capture('settings')

        click(EDIT)
        session.call('wait', '2')
        capture('popup')
        found = profiles()
        checks.check('the editor opens with the default profile',
                     len(found) == 1 and found[0]['name'] == 'Inventory #1', found)

        click(FIELD)
        type_text('diamond')
        session.call('wait', '1')
        capture('typed')
        click(ADD)
        session.call('wait', '2')
        capture('rule-added')
        added = rules()
        checks.check('ADD writes the typed item rule to the profiles setting',
                     len(added) == 1 and 'diamond' in json.dumps(added[0])
                     and added[0]['action'] in ('Drop item', 'Remove', 0),
                     added)

        click(ITEMS_TAB)
        click(RULE)
        session.call('wait', '2')
        capture('rule-cycled')
        cycled = rules()
        checks.check('clicking the rule cycles its action',
                     len(cycled) == 1 and 'Move into inventory' in json.dumps(cycled[0]),
                     cycled)

        click(RULE_X)
        session.call('wait', '2')
        capture('rule-removed')
        checks.check('the rule cross removes it from the setting',
                     rules() == [], rules())

        # The presets tab: NEW creates a preset and its tab, through the same
        # setting.
        click(PRESETS_TAB)
        click(NEW_PRESET)
        session.call('wait', '2')
        capture('preset-new')
        presets = json.loads(session.saved()['settings'].get(MODULE + '.presets', '[]'))
        checks.check('NEW adds a preset to the presets setting',
                     len(presets) == 1 and presets[0]['name'] == 'Rule #1', presets)
        click(CLOSE)
        session.call('wait', '2')
        capture('popup-closed')
        return session.root


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('resources', type=Path)
    args = parser.parse_args()
    run(args.resources)
