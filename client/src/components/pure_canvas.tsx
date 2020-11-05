import React from "react";
import { Component } from "react";
import { processor } from "../services/wasm_runner";

interface PureCanvasProps {
    contextRef: Function;
}

class PureCanvas extends Component<PureCanvasProps, any> {
    shouldComponentUpdate() {
        return false;
    }

    render() {
        return (
            <canvas
                width="160"
                height="144"
                ref={node =>
                    node ? this.props.contextRef(node.getContext('2d')) : null
                }
            />
        );
    }
}


class Engine {
    ctx: CanvasRenderingContext2D;
    frameBuffer: ImageData;
    constructor(ctx: CanvasRenderingContext2D) {
        this.ctx = ctx;
        this.frameBuffer = ctx.createImageData(ctx.canvas.width, ctx.canvas.height);
    }
    tick = () => {
        let p = processor();
        console.log(p.x(), p.y());
        this.ctx.fillRect(p.x(), p.y(), 10, 10);
    };
    drawFrame = () => {
        this.ctx.putImageData(this.frameBuffer, 0, 0);
    };
}

interface GraphicsRendererState {
    engine: Engine | null;
}

export class GraphicsRenderer extends Component<any, GraphicsRendererState> {
    state: GraphicsRendererState = {
        engine: null,
    };

    componentDidMount() {
        requestAnimationFrame(this.updateAnimationState)
    }

    saveContext = (ctx: CanvasRenderingContext2D) => {
        this.setState({
            engine: new Engine(ctx),
        });
    };

    updateAnimationState = () => {
        const { engine } = this.state;
        if (engine) {
            // Draw the last frame we created
            engine.drawFrame();
            // Now let's make a new one!
            engine.tick();
        }
        requestAnimationFrame(this.updateAnimationState);
    };

    render() {
        return <PureCanvas contextRef={this.saveContext} />;
    }
}