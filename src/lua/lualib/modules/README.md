Git-Commit-Id: d89380604bbbc56201d12fd00febde81a647afeb
SPDX-License-Identifier: GPL-2.0-or-later
SPDX-License-Identifier: MIT

The majority of Scribunto is licensed under the GNU General Public License,
version 2 or later.

Some included files and subdirectories are licensed with an MIT license, not
with the GPL. These are:

* package.lua (as described in that file)
* strict.lua (as described in that file)
* luabit/ (as described in readme.txt in that directory)
* ustring/ (as described in README in that directory)

See: https://www.mediawiki.org/wiki/Extension:Scribunto

--

These modules have been slightly modified to support Lua 5.4 style sandboxing
because it is not possible to easily implement the setfenv/getfenv functions in
piccolo. (They can be implemented for entire modules by screwing with upvalues,
but MW relies on also being able to sandbox closures, and this is just not
possible because internally those functions access `_ENV` via lexical scope.)
