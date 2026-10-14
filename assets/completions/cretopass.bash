# bash completion for cretopass
#
# Installed by the cretopass installer to
#   ~/.local/share/bash-completion/completions/cretopass
# and sourced automatically by the bash-completion framework.

_cretopass() {
	local cur prev
	COMPREPLY=()
	cur="${COMP_WORDS[COMP_CWORD]}"
	prev="${COMP_WORDS[COMP_CWORD-1]}"

	# Global options.
	local opts="-h --help -V --version"
	# Subcommands.
	local commands="remove update"

	# Complete an option that expects no argument: nothing further, just
	# stop offering positional guesses.
	case "$prev" in
	-h|--help|-V|--version)
		return 0
		;;
	esac

	case "$cur" in
	-*)
		COMPREPLY=($(compgen -W "$opts" -- "$cur"))
		return 0
		;;
	esac

	# First positional argument: offer subcommands.
	if [[ $COMP_CWORD -eq 1 ]]; then
		COMPREPLY=($(compgen -W "$commands" -- "$cur"))
		return 0
	fi

	# Subcommands take no further arguments today.
	return 0
}

complete -F _cretopass cretopass
