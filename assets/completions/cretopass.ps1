# PowerShell completion for cretopass
#
# Installed by the cretopass installer to
#   <Documents>\PowerShell\Completions\cretopass.ps1
# and dot-sourced from $PROFILE.CurrentUserAllHosts.

Register-ArgumentCompleter -Native -CommandName cretopass -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commands = @(
        [System.Management.Automation.CompletionResult]::new(
            'remove', 'remove', 'ParameterValue',
            'Delete the app and all artifacts from the system')
        [System.Management.Automation.CompletionResult]::new(
            'update', 'update', 'ParameterValue',
            'Check for a new version and install it')
    )
    $options = @(
        [System.Management.Automation.CompletionResult]::new(
            '-h', '-h', 'ParameterName', 'Show help message')
        [System.Management.Automation.CompletionResult]::new(
            '--help', '--help', 'ParameterName', 'Show help message')
        [System.Management.Automation.CompletionResult]::new(
            '-V', '-V', 'ParameterName', 'Show version information')
        [System.Management.Automation.CompletionResult]::new(
            '--version', '--version', 'ParameterName', 'Show version information')
    )

    # Only offer file/subcommand completion for the first argument.
    $elements = $commandAst.CommandElements
    if ($elements.Count -gt 2) {
        return
    }

    if ($wordToComplete -like '-*') {
        $options | Where-Object { $_.CompletionText -like "$wordToComplete*" }
        return
    }

    $commands | Where-Object { $_.CompletionText -like "$wordToComplete*" }
}
