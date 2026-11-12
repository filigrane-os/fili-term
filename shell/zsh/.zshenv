# Aéroport (fili-term): zsh reads this file first (ZDOTDIR set by the session daemon). It puts ZDOTDIR back, so zsh
# goes on with the user's own files, and loads the shell integration for interactive shells.
if [[ -n ${FILI_TERM_ZDOTDIR+set} ]]; then
    ZDOTDIR=$FILI_TERM_ZDOTDIR
    unset FILI_TERM_ZDOTDIR
else
    unset ZDOTDIR
fi
typeset -g _fili_term_dir=${${(%):-%x}:A:h:h}
[[ -r ${ZDOTDIR:-$HOME}/.zshenv ]] && source ${ZDOTDIR:-$HOME}/.zshenv
[[ -o interactive && -r $_fili_term_dir/fili-term.zsh ]] && source $_fili_term_dir/fili-term.zsh
unset _fili_term_dir
