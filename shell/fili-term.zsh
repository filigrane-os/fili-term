# Aéroport (fili-term) shell integration for zsh: marks where each prompt, command line and output starts and how the
# command ended (OSC 133), and where the shell is (OSC 7). Aéroport makes a block of each command from these marks.
# Loaded by Aéroport itself (shell/zsh/.zshenv); can also be sourced from ~/.zshrc.
[[ -o interactive && -z $_fili_term_loaded ]] || return 0
typeset -g _fili_term_loaded=1 _fili_term_running=

# Percent-encoding of $1 into REPLY (bytes outside the safe set)
_fili_term_url() {
    emulate -L zsh
    setopt no_multibyte
    local s=$1 out= c hex
    local -i i
    for (( i = 1; i <= $#s; i++ )); do
        c=$s[i]
        if [[ $c == [A-Za-z0-9/._~-] ]]; then
            out+=$c
        else
            printf -v hex '%%%02X' "'$c"
            out+=$hex
        fi
    done
    REPLY=$out
}

_fili_term_precmd() {
    local ret=$?
    [[ -n $_fili_term_running ]] && printf '\e]133;D;%d\a' $ret
    _fili_term_running=
    _fili_term_url $PWD
    printf '\e]7;file://%s%s\a' ${HOST} $REPLY
    # Marks around the prompt, put back when a theme or the user's files replaced it
    [[ $PS1 == *$'\e]133;A'* ]] || PS1=$'%{\e]133;A\a%}'$PS1
    [[ $PS1 == *$'\e]133;B'* ]] || PS1=$PS1$'%{\e]133;B\a%}'
}

_fili_term_preexec() {
    _fili_term_running=1
    _fili_term_url $1
    printf '\e]133;C;cmdline_url=%s\a' $REPLY
}

# Our precmd goes last at the first prompt, after the hooks the user's files added (prompt themes rewrite PS1)
_fili_term_first() {
    precmd_functions=(${precmd_functions:#_fili_term_first})
    precmd_functions=(${precmd_functions:#_fili_term_precmd} _fili_term_precmd)
    _fili_term_precmd
}
typeset -ag precmd_functions preexec_functions
precmd_functions+=(_fili_term_first)
preexec_functions+=(_fili_term_preexec)
