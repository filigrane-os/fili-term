//! The process in the foreground of a session's terminal: the window offers an integrated mode for a program it knows
//! (herdr) when it sees one there.

/// Process in the foreground of a terminal (the shell itself at its prompt)
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Foreground {
    pub pid: u32,
    pub name: String,
}

impl Foreground {
    /// From `/proc`: the foreground process group of the shell's terminal (`tpgid`, field 8 of `stat`) and its leader's
    /// name. No pty descriptor needed (holding one would keep the shell from its hang-up).
    pub fn of(shell: u32) -> Option<Foreground> {
        let stat = std::fs::read_to_string(format!("/proc/{shell}/stat")).ok()?;
        // The name is in parentheses and may hold spaces: the fields start after the last ')'
        let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
        let tpgid: i64 = fields.get(5)?.parse().ok()?;
        let pid = if tpgid > 0 { tpgid as u32 } else { shell };
        let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?.trim().to_string();
        Some(Foreground { pid, name })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreground_of_this_process() {
        // A test runner may have no terminal: tpgid -1 gives the process itself
        let fg = Foreground::of(std::process::id()).expect("own /proc entry");
        assert!(!fg.name.is_empty());
    }
}
