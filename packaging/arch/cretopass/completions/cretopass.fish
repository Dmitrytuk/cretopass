# fish completion for cretopass

complete -c cretopass -s h -l help -d 'Show help message'
complete -c cretopass -s V -l version -d 'Show version information'

complete -c cretopass -n 'not __fish_seen_subcommand_from remove update' -a remove -d 'Delete the app and all artifacts from the system'
complete -c cretopass -n 'not __fish_seen_subcommand_from remove update' -a update -d 'Check for a new version and install it'