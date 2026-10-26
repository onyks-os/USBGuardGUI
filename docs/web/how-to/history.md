# Review observed events

Open **Main menu → Event history** to see device insertions and removals, authorization outcomes,
reported state changes and reconnections observed while USBGuardGUI is running. The newest
events appear first, and retention is limited to the latest 200 entries.

A marked observation gap means events may have been missed while the connection was unavailable
or a burst required resynchronization. The history describes what this application observed;
it is not a complete daemon audit log.

## Keep history between runs

History normally stays in memory for the current run. To retain it, enable
**Main menu → Preferences → Keep event history between runs**. Turning the setting off removes
the previously saved copy. Only event categories and timestamps are retained: no device names,
serial numbers, device IDs, rule text or daemon error details.

## Export a report

Click **Export privacy-safe JSON…**, choose a file and save. The export contains a schema version,
`identifiers_included: false`, and event records with timestamps and category names. It excludes
device identifiers whether persistence is enabled or disabled.
