# bash completion for cretopass
# cretopass currently accepts no command-line arguments.
# Extend this file when CLI options are added.
_cretopass() {
	local cur
	COMPREPLY=()
	cur="${COMP_WORDS[COMP_CWORD]}"

	case "$cur" in
	-*)
		COMPREPLY=($(compgen -W "" -- "$cur"))
		;;
	esac
}
complete -F _cretopass cretopass
