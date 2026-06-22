/**
 * Daily Notes Plugin
 * Automatically creates and manages daily notes
 */

class DailyNotesPlugin {
    constructor(api) {
        this.api = api;
        this.config = {};
    }

    async onLoad(ctx) {
        console.log('Daily Notes plugin loaded', ctx);

        // Load configuration
        const savedConfig = await this.api.storage_get('config');
        this.config = savedConfig || this.getDefaultConfig();

        // Register commands
        await this.registerCommands();

        // Add ribbon icon
        this.api.addRibbonIcon('calendar', 'Open today\'s note', async () => {
            await this.openTodaysNote();
        });

        console.log('Daily Notes plugin initialized');
    }

    async onStartup() {
        if (this.config.open_on_startup) {
            await this.openTodaysNote();
        }
    }

    async onUnload() {
        console.log('Daily Notes plugin unloaded');
    }

    getDefaultConfig() {
        return {
            daily_notes_folder: 'Daily Notes',
            date_format: 'YYYY-MM-DD',
            template_file: 'Templates/Daily Note.md',
            open_on_startup: true
        };
    }

    async registerCommands() {
        // Open today's note
        await this.api.register_command({
            id: 'open-today',
            name: 'Daily Notes: Open Today',
            description: 'Open or create today\'s daily note',
            hotkey: 'Ctrl+Shift+D'
        });

        // Open yesterday's note
        await this.api.register_command({
            id: 'open-yesterday',
            name: 'Daily Notes: Open Yesterday',
            description: 'Open yesterday\'s daily note'
        });

        // Open tomorrow's note
        await this.api.register_command({
            id: 'open-tomorrow',
            name: 'Daily Notes: Open Tomorrow',
            description: 'Open or create tomorrow\'s daily note'
        });
    }

    async openTodaysNote() {
        const today = this.formatDate(new Date());
        await this.openDailyNote(today);
    }

    async openDailyNote(dateStr) {
        const folder = this.config.daily_notes_folder;
        const filePath = `${folder}/${dateStr}.md`;

        try {
            // Try to read existing note
            const content = await this.api.read_file(this.api.getContext().vault_id, filePath);
            console.log('Opened existing daily note:', filePath);
        } catch (error) {
            // Note doesn't exist, create it
            await this.createDailyNote(dateStr, filePath);
        }
    }

    async createDailyNote(dateStr, filePath) {
        let content = '';

        // Try to load template
        try {
            const template = await this.api.read_file(
                this.api.getContext().vault_id,
                this.config.template_file
            );
            content = this.processTemplate(template, dateStr);
        } catch (error) {
            // No template, use default
            content = this.getDefaultTemplate(dateStr);
        }

        // Create the note
        await this.api.write_file(
            this.api.getContext().vault_id,
            filePath,
            content
        );

        await this.api.show_notice(`Created daily note: ${dateStr}`);
        console.log('Created daily note:', filePath);
    }

    processTemplate(template, dateStr) {
        const date = new Date(dateStr);

        return template
            .replace(/{{date}}/g, dateStr)
            .replace(/{{day}}/g, date.toLocaleDateString('en-US', { weekday: 'long' }))
            .replace(/{{time}}/g, new Date().toLocaleTimeString())
            .replace(/{{year}}/g, date.getFullYear().toString())
            .replace(/{{month}}/g, (date.getMonth() + 1).toString().padStart(2, '0'))
            .replace(/{{day-num}}/g, date.getDate().toString().padStart(2, '0'));
    }

    getDefaultTemplate(dateStr) {
        return `# ${dateStr}

## Tasks
- [ ] 

## Notes


## Reflections

`;
    }

    formatDate(date) {
        const fmt = (this.config && this.config.date_format) || 'YYYY-MM-DD';

        const pad = (n) => String(n).padStart(2, '0');

        const MONTHS_LONG  = ['January','February','March','April','May','June',
                              'July','August','September','October','November','December'];
        const MONTHS_SHORT = ['Jan','Feb','Mar','Apr','May','Jun',
                              'Jul','Aug','Sep','Oct','Nov','Dec'];
        const DAYS_LONG    = ['Sunday','Monday','Tuesday','Wednesday','Thursday','Friday','Saturday'];
        const DAYS_SHORT   = ['Sun','Mon','Tue','Wed','Thu','Fri','Sat'];

        const YYYY  = date.getFullYear();
        const YY    = String(YYYY).slice(-2);
        const M     = date.getMonth() + 1;
        const MM    = pad(M);
        const D     = date.getDate();
        const DD    = pad(D);
        const MMMM  = MONTHS_LONG[date.getMonth()];
        const MMM   = MONTHS_SHORT[date.getMonth()];
        const dddd  = DAYS_LONG[date.getDay()];
        const ddd   = DAYS_SHORT[date.getDay()];
        const HH    = pad(date.getHours());
        const hh    = pad(date.getHours() % 12 || 12);
        const mm    = pad(date.getMinutes());
        const ss    = pad(date.getSeconds());

        // Replace tokens longest-first to avoid partial matches (e.g. MMMM before MM).
        return fmt
            .replace(/YYYY/g, YYYY)
            .replace(/YY/g,   YY)
            .replace(/MMMM/g, MMMM)
            .replace(/MMM/g,  MMM)
            .replace(/MM/g,   MM)
            .replace(/M(?!M)/g, M)
            .replace(/DD/g,   DD)
            .replace(/D(?!D)/g, D)
            .replace(/dddd/g, dddd)
            .replace(/ddd/g,  ddd)
            .replace(/HH/g,   HH)
            .replace(/hh/g,   hh)
            .replace(/mm/g,   mm)
            .replace(/ss/g,   ss);
    }
}

// Export plugin class
export default DailyNotesPlugin;
