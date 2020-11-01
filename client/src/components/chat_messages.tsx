import React from "react";
import { processor as p } from '../services/wasm_runner';

export class ChatMessages extends React.Component<{}, { value: Array<string> }> {
    constructor(props: any) {
        super(props);
        this.state = { value: [] };
    }

    componentDidMount() {
        let self = this;
        setInterval(async function () {
            let processor = p();
            let pending = processor.get_pending();
            if (Array.isArray(pending) && Array.length) {
                self.setState({ value: self.state.value.concat(pending) })
            }
        }, 100);
    }

    render() {
        return (
            <ul>
                {
                    this.state.value.map((item, index) => (
                        <li key={item + index}>{item}</li>
                    ))
                }
            </ul>
        );
    }
}