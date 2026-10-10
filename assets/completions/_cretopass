#compdef cretopass
# zsh completion for cretopass

_cretopass() {
	_arguments \
		'(-h --help)'{-h,--help}'[show help message]' \
		'(-V --version)'{-V,--version}'[show version information]' \
		'1:command:_cretopass_commands'
}

_cretopass_commands() {
	local -a commands
	commands=(
		'remove:delete the app and all artifacts from the system'
		'update:check for a new version and install it'
	)
	_describe -t commands 'command' commands
}

_cretopass "$@"