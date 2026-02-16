export interface SlashCommandConfig {
    description: string;
    replacement?: string;
}

export const SLASH_COMMANDS: Record<string, SlashCommandConfig> = {
    '/shrug': { description: 'Sends ¯\\_(ツ)_/¯', replacement: '¯\\_(ツ)_/¯' },
    '/tableflip': { description: 'Sends (╯°□°)╯︵ ┻━┻', replacement: '(╯°□°)╯︵ ┻━┻' },
    '/unflip': { description: 'Sends ┬─┬ノ( º _ ºノ)', replacement: '┬─┬ノ( º _ ºノ)' },
    '/lenny': { description: 'Sends ( ͡° ͜ʖ ͡°)', replacement: '( ͡° ͜ʖ ͡°)' },
    '/disapprove': { description: 'Sends ಠ_ಠ', replacement: 'ಠ_ಠ' },
    '/clear': { description: 'Clear local chat view' },
    '/deleteall': { description: 'Delete entire conversation (server)' },
};

export const getMatchingSlashCommands = (input: string): Array<[string, SlashCommandConfig]> => {
    if (!input.startsWith('/')) {
        return [];
    }

    return Object.entries(SLASH_COMMANDS)
        .filter(([cmd]) => cmd.startsWith(input.toLowerCase()))
        .slice(0, 5);
};
