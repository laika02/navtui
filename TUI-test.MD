# TUI Test Mockup

Replica of `TUI.MD` reference mock with a candidate volume slider style.

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
|VOL [||||||||||||||......] 70%                           |
+---------------------------------------------------------+
+- Queue -------------------------------------------------+
|Nedaj - GENESIS                                          |
|Nedaj • Sadbalmain - MORTEM                              |
|                                                         |
+---------------------------------------------------------+
```

## Volume Slider Candidate

- Style: `VOL [||||||||||||||......] 70%`
- Filled segment (`|`) represents current volume.
- Empty segment (`.`) represents remaining range.
- Fixed-width bar keeps the UI stable while volume changes.

## Alternative Slider Styles

- `VOL [##############------] 70%`
- `VOL [==============      ] 70%`
- `VOL 70% |■■■■■■■■■■■■■■□□□□□□|`
- `VOL [14/20] (70%)`

## Timeline Scrubber Draft

Candidate inline style for Now Playing:

```text
+- Now Playing -++---------------------------------------+
|Nedaj - Code                                             |
|VOL 70% |■■■■■■■■■■■■■■□□□□□□|                           |
|POS 01:23 |■■■■■■■■□□□□□□□| 03:45                        |
+---------------------------------------------------------+
```

Alternative with a draggable caret:

```text
POS 01:23 |■■■■■■■■□□□□□□□| 03:45
                 ^
```

- `POS` line shows elapsed and total time.
- Filled segment (`■`) is played portion, empty (`□`) is remaining.
- Caret (`^`) can represent current drag target while scrubbing with mouse.
