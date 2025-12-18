# Low-Priority Research (Resource-Intensive)

These topics require significant compute resources (GPU time, training infrastructure) or are less immediately actionable for moss. Explore when resources permit or if moss gains traction that justifies the investment.

## Inference & Optimization

Topics related to making LLM inference faster/cheaper. Useful if moss scales to high-volume usage.

- [ ] Speculative decoding for faster code generation
- [ ] Model quantization trade-offs for code quality
- [ ] Batching strategies for multi-file operations
- [ ] Caching strategies for repeated queries
- [ ] KV cache optimization for long contexts
- [ ] Continuous batching for concurrent requests

## Fine-Tuning & Adaptation

Topics that require training infrastructure and datasets.

- [ ] LoRA for code-specific tasks
- [ ] Instruction tuning for coding agents
- [ ] Domain adaptation (finance, healthcare, etc.)
- [ ] Continual learning from user feedback
- [ ] Reward modeling for code quality
- [ ] RLHF/DPO for coding preferences

## Model Training

Even more resource-intensive - training from scratch or significant fine-tuning.

- [ ] Pre-training on codebase-specific data
- [ ] Multi-task training (synthesis + repair + review)
- [ ] Code-specific tokenizer optimization
- [ ] Mixture of experts for different languages

## When to Prioritize These

Consider moving items to main TODO.md when:
- Moss has production users with scale requirements
- Specific performance bottlenecks are identified
- Funding/resources become available for training
- A specific use case requires fine-tuned behavior
