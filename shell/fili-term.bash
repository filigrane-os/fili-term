# Aéroport (fili-term) shell integration for bash: marks where each prompt, command line and output starts and how the
# command ended (OSC 133), and where the shell is (OSC 7). Aéroport starts bash with this file as its rc file (it reads
# ~/.bashrc first, like bash would); it can also be sourced from ~/.bashrc. Needs bash 4.4 (PS0).
if [[ -z $_fili_term_loaded && $- == *i* ]]; then
    if [[ -n $FILI_TERM_SESSION && -z $_fili_term_rc ]]; then
        _fili_term_rc=1
        [[ -r ~/.bashrc ]] && source ~/.bashrc
    fi
    _fili_term_loaded=1
    _fili_term_running=

    _fili_term_url() {
        local LC_ALL=C s=$1 out= c i
        for (( i = 0; i < ${#s}; i++ )); do
            c=${s:i:1}
            case $c in
                [A-Za-z0-9/._~-]) out+=$c ;;
                *) printf -v c '%%%02X' "'$c"; out+=$c ;;
            esac
        done
        REPLY=$out
    }

    _fili_term_prompt() {
        local ret=$?
        [[ -n $_fili_term_running ]] && printf '\e]133;D;%d\a' "$ret"
        _fili_term_running=
        _fili_term_url "$PWD"
        printf '\e]7;file://%s%s\a' "$HOSTNAME" "$REPLY"
        [[ $PS1 == *'133;A'* ]] || PS1='\[\e]133;A\a\]'$PS1
        [[ $PS1 == *'133;B'* ]] || PS1=$PS1'\[\e]133;B\a\]'
        return $ret
    }

    # PS0 is printed once the command line is read, before it runs. Its $(…) runs in a subshell: the running flag is
    # set by the arithmetic of an array index, which the shell itself evaluates (and which expands to nothing)
    _fili_term_ps0() {
        _fili_term_url "$(HISTTIMEFORMAT= history 1 | sed 's/^ *[0-9]* *//')"
        printf '\e]133;C;cmdline_url=%s\a' "$REPLY"
    }
    _fili_term_none=()
    PS0='${_fili_term_none[_fili_term_running=1]}$(_fili_term_ps0)'$PS0
    PROMPT_COMMAND="_fili_term_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
