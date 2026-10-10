# PowerShell completion for cretopass
# TODO: complete argument completion for cretopass.exe

Register-ArgumentCompleter -Native -CommandName cretopass -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commands = @('remove', 'update')
    $options = @('-h', '--help', '-V', '--version')

    $all = $commands + $options | Where-Object { $_ -like "$wordToComplete*" }
    $all | ForEach-Object {
        [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)
    }
}