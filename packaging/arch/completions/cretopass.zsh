#compdef cretopass
# zsh completion for cretopass
# cretopass currently accepts no command-line arguments.
# Extend this file when CLI options are added.

_cretopass() {
	_arguments \
		'(-h --help)'{-h,--help}'[show help message]' \
		'(-V --version)'{-V,--version}'[show version information]'
}

_cretopass "$@"
