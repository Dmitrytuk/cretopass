# bash completion for cretopass
_cretopass() {
	local cur prev
	COMPREPLY=()
	cur="${COMP_WORDS[COMP_CWORD]}"
	prev="${COMP_WORDS[COMP_CWORD-1]}"

	case "$cur" in
	-*)
		COMPREPLY=($(compgen -W "-h --help -V --version" -- "$cur"))
		;;
	*)
		if [[ $COMP_CWORD -eq 1 ]]; then
			COMPREPLY=($(compgen -W "remove update" -- "$cur"))
		fi
		;;
	esac
}
complete -F _cretopass cretopass