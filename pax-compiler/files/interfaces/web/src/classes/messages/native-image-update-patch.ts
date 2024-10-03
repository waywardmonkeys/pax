export class NativeImageUpdatePatch {
    public id?: number;
    public url?: string;
    public fit?: string;
    public size_x?: number;
    public size_y?: number;
    public transform?: number[];

    fromPatch(jsonMessage: any) {
        this.id = jsonMessage["id"];
        this.url = jsonMessage["url"];
        this.fit = jsonMessage["fit"];
        this.size_x = jsonMessage["size_x"];
        this.size_y = jsonMessage["size_y"];
        this.transform = jsonMessage["transform"];
    }

    cleanUp(){
        this.id = undefined;
        this.url = '';
        this.fit = '';
        this.size_x = 0;
        this.size_y = 0;
        this.transform = [];
    }
}