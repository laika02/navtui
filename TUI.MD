# TUI Layout Contract

Primary screen has three vertical sections:

1. Main browser pane (Artists / Albums / Songs tabs)
2. Now Playing pane
3. Queue + status pane

Sizing behavior:

- `Now Playing` keeps a compact fixed height.
- On taller terminals, extra vertical space is shared proportionally, with a moderate bias to the main browser pane and meaningful growth for `Queue`.
- Main pane header shows tab/search state; `username@server-host` can be shown at the bottom-left of the Queue pane (`show_identity_label` config option).
- `Now Playing` contains track state/title, volume slider, and a timeline scrubber line (`POS elapsed |■■■□□□| total`) below volume.

Reference mock:

```text
+-[Artists]- Albums - Songs ------------------------------+
|$uicideboy$                                              |
|$uicideboy$/Germ                                         |
|$uicideboy$/Getter                                       |
|100 gecs                                                 |
|556Kurumi                                                |
|...                                                      |
+---------------------------------------------------------+
++- Now Playing -++---------------------------------------+
|Nedaj - Code                                             |
+---------------------------------------------------------+
+- Queue -------------------------------------------------+
|Nedaj - GENESIS                                          |
|Nedaj • Sadbalmain - MORTEM                              |
|                                                         |
+---------------------------------------------------------+
```

## Interaction Contract

The bindings listed below are defaults. All keyboard bindings are configurable via `config.toml` `[keybinds]`.

Tabs and navigation:

- `Tab`: cycle `Artists -> Albums -> Songs` and reset context/filter state
- `1`, `2`, `3`: jump to Artists, Albums, Songs (same reset behavior)
- `Shift` (or `Shift+Tab` fallback): toggle Queue Focus mode
- `Up`/`Down`: move selection, wrapping at boundaries
- Holding arrow keys repeats movement/navigation in that direction
- `Right` or `Enter`:
  - Artists tab: open Albums scoped to selected artist
  - Albums tab: open Songs scoped to selected album
  - Songs tab: play selected song
- `Left`: back to previous drill-down context

Playback and queue:

- `Space`: play/pause current track
- `[` / `]`: volume down / up
- `;` / `'`: rewind / fast-forward 10 seconds
- `s`: shuffle context
  - Artists tab: selected artist discography
  - Albums tab: selected album songs
  - Songs tab: global songs
- `a` or `A`: enqueue selected item depending on active tab
  - Artists tab: selected artist songs
  - Albums tab: selected album songs
  - Songs tab: selected song
  - If queue is idle, first enqueue also starts playback
- `n`: play-next selected item depending on active tab
  - Artists tab: selected artist songs
  - Albums tab: selected album songs
  - Songs tab: selected song
  - In Queue Focus mode: selected queue song is moved to play next
- `c`: clear queue and stop playback
- `r`: hard refresh library from server (clear disk caches first) and reset browser state
- `Shift+Esc`: global reset to main Artists tab (clears search + filters/scopes)
- `Esc`: clear search/filter first; if already clear, acts as back
- `,`: queue back
- `.`: queue forward
- `q`: quit
- Mouse: click/drag Now Playing volume slider to set volume
- Mouse: click/drag Now Playing timeline scrubber to seek position
- Mouse: wheel scroll over main browser pane moves tab selection (`Up`/`Down` semantics)
- Mouse: wheel scroll over Queue pane scrolls queue viewport
- Queue viewport auto-follows current item when queue position changes (`,`, `.`, auto-advance, initial enqueue start)

Queue Focus mode:

- `Up`/`Down`: move queue selection cursor (wrapping)
- `Enter`/`Right`: play selected queue item
- `Backspace`: remove selected queue item
- `Space`: pick/drop selected queue item for reordering
  - While picked, `Up`/`Down` reorders that item in the queue
- `Esc`: cancel active reorder and restore queue order to its pre-move state
- Browser-pane keys are disabled while queue mode is active

Search:

- `/`: start search in active tab
- Live case-insensitive filtering as you type
- `Backspace` on empty search: exits search mode
- `Esc`: clear search/filter; if already clear, leave search and go back
- While searching, `Up`/`Down`/`Left` still navigate
- While searching, `Tab` cancels search then switches tabs
- While searching, `1/2/3` are regular search input
- If current tab has an active filter, `Backspace` keeps editing it even outside explicit search mode
- If filter narrows to a single item, it is auto-selected
  - Optional: with `expand_on_search_collapse = true`, single-match in Artists/Albums auto-expands and clears search
