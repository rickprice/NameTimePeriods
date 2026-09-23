# NameTimePeriod

A simple and extensible command-line tool written in Rust to determine which named time period (like "Mother's Day" or "Easter") a given date falls into, based on configurable YAML definitions.

## Features

- Supports flexible date definitions like:
  - `The second Sunday of May`
  - `Easter`, `Thanksgiving`, `LaborDay`, `MemorialDay`, `MLKDay`
- Configurable `DaysBefore` and `DaysAfter` buffer windows
- All matching periods are output, space-separated (e.g. `MothersDay EasterPeriod`)
- System (`/etc`) and user (`~/.config`) config files, merged at runtime
- Command-line override of the date being tested
- Auto-generates a default user config on first run; `--init` to force-regenerate

## Installation

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) 1.88 or later

### Clone and build

```bash
git clone https://github.com/rickprice/NameTimePeriods.git
cd NameTimePeriods
cargo build --release
```

## Usage

```bash
name_time_period                    # check today's date
name_time_period --date 2025-05-11  # check a specific date
name_time_period --init             # force-(re)create user config
```

## Configuration

### Config file locations

| Priority | Path |
|----------|------|
| User (checked first) | `~/.config/NameTimePeriod/time_periods.yaml` |
| System | `/etc/NameTimePeriod/time_periods.yaml` |

Both files are loaded and merged. The user config is created automatically on first run if neither file exists.

### Example `time_periods.yaml`

```yaml
TimePeriods:
  - MothersDay:
      Date: The second Sunday of May
      DaysBefore: 3
      DaysAfter: 1
      Comment: Mother's Day
  - EasterPeriod:
      Date: Easter
      DaysBefore: 5
      DaysAfter: 2
  - Christmas:
      Date: December 25
      DaysBefore: 3
      DaysAfter: 1
```

### Supported `Date` values

| Value | Meaning |
|-------|---------|
| `Easter` | Western Easter Sunday (anonymous Gregorian algorithm) |
| `Thanksgiving` | 4th Thursday of November |
| `LaborDay` | 1st Monday of September |
| `MemorialDay` | Last Monday of May |
| `MLKDay` | 3rd Monday of January |
| `The N Weekday of Month` | e.g. `The second Sunday of May` |
| `Month DD` | e.g. `December 25` |

## Running Tests

```bash
cargo test
```

The test suite covers Easter calculations across multiple years, all weekday/ordinal
combinations, boundary conditions for period matching, and YAML deserialization.

## FAQ

**Q: What if today matches more than one period?**  
A: All matching period names are printed, space-separated. Output `Default` if nothing matches.

**Q: Can I define custom holidays?**  
A: Yes — add them to your user YAML using any supported date format.

**Q: What if the same entry appears in both config files?**  
A: Both are loaded; if they both match, both names appear in the output.

## License

MIT — see `LICENSE` for details.

## Author

Frederick Price
